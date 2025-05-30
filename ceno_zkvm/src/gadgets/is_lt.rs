use std::{fmt::Display, mem::MaybeUninit};

use ceno_emul::{SWord, Word};
use ff_ext::ExtensionField;
use goldilocks::SmallField;
use itertools::izip;

use crate::{
    Value,
    chip_handler::utils::power_sequence,
    circuit_builder::CircuitBuilder,
    error::ZKVMError,
    expression::{Expression, ToExpr, WitIn},
    instructions::riscv::constants::{UINT_LIMBS, UInt},
    set_val,
    witness::LkMultiplicity,
};

use super::SignedExtendConfig;

#[derive(Debug, Clone)]
pub struct AssertLTConfig(InnerLtConfig);

impl AssertLTConfig {
    pub fn construct_circuit<
        E: ExtensionField,
        NR: Into<String> + Display + Clone,
        N: FnOnce() -> NR,
    >(
        cb: &mut CircuitBuilder<E>,
        name_fn: N,
        lhs: Expression<E>,
        rhs: Expression<E>,
        max_num_u16_limbs: usize,
    ) -> Result<Self, ZKVMError> {
        cb.namespace(
            || "assert_lt",
            |cb| {
                let name = name_fn();
                let config = InnerLtConfig::construct_circuit(
                    cb,
                    name,
                    lhs,
                    rhs,
                    Expression::ONE,
                    max_num_u16_limbs,
                )?;
                Ok(Self(config))
            },
        )
    }

    pub fn assign_instance<F: SmallField>(
        &self,
        instance: &mut [MaybeUninit<F>],
        lkm: &mut LkMultiplicity,
        lhs: u64,
        rhs: u64,
    ) -> Result<(), ZKVMError> {
        self.0.assign_instance(instance, lkm, lhs, rhs)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IsLtConfig {
    pub is_lt: WitIn,
    config: InnerLtConfig,
}

impl IsLtConfig {
    pub fn expr<E: ExtensionField>(&self) -> Expression<E> {
        self.is_lt.expr()
    }

    pub fn construct_circuit<
        E: ExtensionField,
        NR: Into<String> + Display + Clone,
        N: FnOnce() -> NR,
    >(
        cb: &mut CircuitBuilder<E>,
        name_fn: N,
        lhs: Expression<E>,
        rhs: Expression<E>,
        max_num_u16_limbs: usize,
    ) -> Result<Self, ZKVMError> {
        cb.namespace(
            || "is_lt",
            |cb| {
                let name = name_fn();
                let is_lt = cb.create_witin(|| format!("{name} is_lt witin"));
                cb.assert_bit(|| "is_lt_bit", is_lt.expr())?;

                let config = InnerLtConfig::construct_circuit(
                    cb,
                    name,
                    lhs,
                    rhs,
                    is_lt.expr(),
                    max_num_u16_limbs,
                )?;
                Ok(Self { is_lt, config })
            },
        )
    }

    pub fn assign_instance<F: SmallField>(
        &self,
        instance: &mut [MaybeUninit<F>],
        lkm: &mut LkMultiplicity,
        lhs: u64,
        rhs: u64,
    ) -> Result<(), ZKVMError> {
        set_val!(instance, self.is_lt, (lhs < rhs) as u64);
        self.config.assign_instance(instance, lkm, lhs, rhs)?;
        Ok(())
    }

    pub fn assign_instance_signed<F: SmallField>(
        &self,
        instance: &mut [MaybeUninit<F>],
        lkm: &mut LkMultiplicity,
        lhs: SWord,
        rhs: SWord,
    ) -> Result<(), ZKVMError> {
        set_val!(instance, self.is_lt, (lhs < rhs) as u64);
        self.config
            .assign_instance_signed(instance, lkm, lhs, rhs)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct InnerLtConfig {
    pub diff: Vec<WitIn>,
    pub max_num_u16_limbs: usize,
}

impl InnerLtConfig {
    fn range(max_num_u16_limbs: usize) -> u64 {
        1u64 << (u16::BITS as usize * max_num_u16_limbs)
    }

    pub fn construct_circuit<E: ExtensionField, NR: Into<String> + Display + Clone>(
        cb: &mut CircuitBuilder<E>,
        name: NR,
        lhs: Expression<E>,
        rhs: Expression<E>,
        is_lt_expr: Expression<E>,
        max_num_u16_limbs: usize,
    ) -> Result<Self, ZKVMError> {
        assert!(max_num_u16_limbs >= 1);

        let mut witin_u16 = |var_name: String| -> Result<WitIn, ZKVMError> {
            cb.namespace(
                || format!("var {var_name}"),
                |cb| {
                    let witin = cb.create_witin(|| var_name.to_string());
                    cb.assert_ux::<_, _, 16>(|| name.clone(), witin.expr())?;
                    Ok(witin)
                },
            )
        };

        let diff = (0..max_num_u16_limbs)
            .map(|i| witin_u16(format!("diff_{i}")))
            .collect::<Result<Vec<WitIn>, _>>()?;

        let pows = power_sequence((1 << u16::BITS).into());

        let diff_expr = izip!(&diff, pows)
            .map(|(record, beta)| beta * record.expr())
            .sum::<Expression<E>>();

        let range = Self::range(max_num_u16_limbs);

        cb.require_equal(|| name.clone(), lhs - rhs, diff_expr - is_lt_expr * range)?;

        Ok(Self {
            diff,
            max_num_u16_limbs,
        })
    }

    pub fn assign_instance<F: SmallField>(
        &self,
        instance: &mut [MaybeUninit<F>],
        lkm: &mut LkMultiplicity,
        lhs: u64,
        rhs: u64,
    ) -> Result<(), ZKVMError> {
        let diff = cal_lt_diff(lhs < rhs, self.max_num_u16_limbs, lhs, rhs);
        self.diff.iter().enumerate().for_each(|(i, wit)| {
            // extract the 16 bit limb from diff and assign to instance
            let val = (diff >> (i * u16::BITS as usize)) & 0xffff;
            lkm.assert_ux::<16>(val);
            set_val!(instance, wit, val);
        });
        Ok(())
    }

    // TODO: refactor with the above function
    pub fn assign_instance_signed<F: SmallField>(
        &self,
        instance: &mut [MaybeUninit<F>],
        lkm: &mut LkMultiplicity,
        lhs: SWord,
        rhs: SWord,
    ) -> Result<(), ZKVMError> {
        let diff = if lhs < rhs {
            Self::range(self.diff.len()) - lhs.abs_diff(rhs) as u64
        } else {
            lhs.abs_diff(rhs) as u64
        };
        self.diff.iter().enumerate().for_each(|(i, wit)| {
            // extract the 16 bit limb from diff and assign to instance
            let val = (diff >> (i * u16::BITS as usize)) & 0xffff;
            lkm.assert_ux::<16>(val);
            set_val!(instance, wit, val);
        });
        Ok(())
    }
}

pub fn cal_lt_diff(is_lt: bool, max_num_u16_limbs: usize, lhs: u64, rhs: u64) -> u64 {
    (if is_lt {
        1u64 << (u16::BITS as usize * max_num_u16_limbs)
    } else {
        0
    } + lhs
        - rhs)
}

#[derive(Debug)]
pub struct AssertSignedLtConfig<E> {
    config: InnerSignedLtConfig<E>,
}

impl<E: ExtensionField> AssertSignedLtConfig<E> {
    pub fn construct_circuit<NR: Into<String> + Display + Clone, N: FnOnce() -> NR>(
        cb: &mut CircuitBuilder<E>,
        name_fn: N,
        lhs: &UInt<E>,
        rhs: &UInt<E>,
    ) -> Result<Self, ZKVMError> {
        cb.namespace(
            || "assert_signed_lt",
            |cb| {
                let name = name_fn();
                let config =
                    InnerSignedLtConfig::construct_circuit(cb, name, lhs, rhs, Expression::ONE)?;
                Ok(Self { config })
            },
        )
    }

    pub fn assign_instance(
        &self,
        instance: &mut [MaybeUninit<E::BaseField>],
        lkm: &mut LkMultiplicity,
        lhs: SWord,
        rhs: SWord,
    ) -> Result<(), ZKVMError> {
        self.config.assign_instance(instance, lkm, lhs, rhs)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct SignedLtConfig<E> {
    is_lt: WitIn,
    config: InnerSignedLtConfig<E>,
}

impl<E: ExtensionField> SignedLtConfig<E> {
    pub fn expr(&self) -> Expression<E> {
        self.is_lt.expr()
    }

    pub fn construct_circuit<NR: Into<String> + Display + Clone, N: FnOnce() -> NR>(
        cb: &mut CircuitBuilder<E>,
        name_fn: N,
        lhs: &UInt<E>,
        rhs: &UInt<E>,
    ) -> Result<Self, ZKVMError> {
        cb.namespace(
            || "is_signed_lt",
            |cb| {
                let name = name_fn();
                let is_lt = cb.create_witin(|| format!("{name} is_signed_lt witin"));
                cb.assert_bit(|| "is_lt_bit", is_lt.expr())?;
                let config =
                    InnerSignedLtConfig::construct_circuit(cb, name, lhs, rhs, is_lt.expr())?;

                Ok(SignedLtConfig { is_lt, config })
            },
        )
    }

    pub fn assign_instance(
        &self,
        instance: &mut [MaybeUninit<E::BaseField>],
        lkm: &mut LkMultiplicity,
        lhs: SWord,
        rhs: SWord,
    ) -> Result<(), ZKVMError> {
        set_val!(instance, self.is_lt, (lhs < rhs) as u64);
        self.config
            .assign_instance(instance, lkm, lhs as SWord, rhs as SWord)?;
        Ok(())
    }
}

#[derive(Debug)]
struct InnerSignedLtConfig<E> {
    is_lhs_neg: SignedExtendConfig<E>,
    is_rhs_neg: SignedExtendConfig<E>,
    config: InnerLtConfig,
}

impl<E: ExtensionField> InnerSignedLtConfig<E> {
    pub fn construct_circuit<NR: Into<String> + Display + Clone>(
        cb: &mut CircuitBuilder<E>,
        name: NR,
        lhs: &UInt<E>,
        rhs: &UInt<E>,
        is_lt_expr: Expression<E>,
    ) -> Result<Self, ZKVMError> {
        // Extract the sign bit.
        let is_lhs_neg = lhs.is_negative(cb)?;
        let is_rhs_neg = rhs.is_negative(cb)?;

        // Convert to field arithmetic.
        let lhs_value = lhs.to_field_expr(is_lhs_neg.expr());
        let rhs_value = rhs.to_field_expr(is_rhs_neg.expr());
        let config = InnerLtConfig::construct_circuit(
            cb,
            format!("{name} (lhs < rhs)"),
            lhs_value,
            rhs_value,
            is_lt_expr,
            UINT_LIMBS,
        )?;

        Ok(Self {
            is_lhs_neg,
            is_rhs_neg,
            config,
        })
    }

    pub fn assign_instance(
        &self,
        instance: &mut [MaybeUninit<E::BaseField>],
        lkm: &mut LkMultiplicity,
        lhs: SWord,
        rhs: SWord,
    ) -> Result<(), ZKVMError> {
        let lhs_value = Value::new_unchecked(lhs as Word);
        let rhs_value = Value::new_unchecked(rhs as Word);
        self.is_lhs_neg.assign_instance(
            instance,
            lkm,
            *lhs_value.as_u16_limbs().last().unwrap() as u64,
        )?;
        self.is_rhs_neg.assign_instance(
            instance,
            lkm,
            *rhs_value.as_u16_limbs().last().unwrap() as u64,
        )?;

        self.config
            .assign_instance_signed(instance, lkm, lhs, rhs)?;
        Ok(())
    }
}
