use ceno_emul::{InsnKind, StepRecord};
use ff_ext::ExtensionField;

use super::{
    RIVInstruction,
    constants::{UINT_LIMBS, UInt},
    r_insn::RInstructionConfig,
};
use crate::{
    circuit_builder::CircuitBuilder,
    error::ZKVMError,
    expression::Expression,
    gadgets::{IsLtConfig, IsZeroConfig},
    instructions::Instruction,
    uint::Value,
    witness::LkMultiplicity,
};
use core::mem::MaybeUninit;
use std::marker::PhantomData;

pub struct ArithConfig<E: ExtensionField> {
    r_insn: RInstructionConfig<E>,

    dividend: UInt<E>,
    divisor: UInt<E>,
    pub(crate) outcome: UInt<E>,

    remainder: UInt<E>,
    inter_mul_value: UInt<E>,
    is_zero: IsZeroConfig,
    pub remainder_lt: IsLtConfig,
}

pub struct ArithInstruction<E, I>(PhantomData<(E, I)>);

pub struct DivUOp;
impl RIVInstruction for DivUOp {
    const INST_KIND: InsnKind = InsnKind::DIVU;
}
pub type DivUInstruction<E> = ArithInstruction<E, DivUOp>;

impl<E: ExtensionField, I: RIVInstruction> Instruction<E> for ArithInstruction<E, I> {
    type InstructionConfig = ArithConfig<E>;

    fn name() -> String {
        format!("{:?}", I::INST_KIND)
    }

    fn construct_circuit(cb: &mut CircuitBuilder<E>) -> Result<Self::InstructionConfig, ZKVMError> {
        // outcome = dividend / divisor + remainder => dividend = divisor * outcome + r
        let mut divisor = UInt::new_unchecked(|| "divisor", cb)?;
        let mut outcome = UInt::new(|| "outcome", cb)?;
        let r = UInt::new(|| "remainder", cb)?;
        let (dividend, inter_mul_value) =
            divisor.mul_add(|| "divisor * outcome + r", cb, &mut outcome, &r, true)?;

        // div by zero check
        let is_zero =
            IsZeroConfig::construct_circuit(cb, || "divisor_zero_check", divisor.value())?;
        let outcome_value = outcome.value();
        cb.condition_require_equal(
            || "outcome_is_zero",
            is_zero.expr(),
            outcome_value.clone(),
            ((1u64 << UInt::<E>::TOTAL_BITS) - 1).into(),
            outcome_value,
        )?;

        // remainder should be less than divisor if divisor != 0.
        let lt = IsLtConfig::construct_circuit(
            cb,
            || "remainder < divisor?",
            r.value(),
            divisor.value(),
            UINT_LIMBS,
        )?;

        // When divisor is zero, remainder is -1 implies "remainder > divisor" aka. lt.expr() == 0
        // otherwise lt.expr() == 1
        cb.require_equal(
            || "remainder < divisor when non-zero divisor",
            is_zero.expr() + lt.expr(),
            Expression::ONE,
        )?;

        let r_insn = RInstructionConfig::<E>::construct_circuit(
            cb,
            I::INST_KIND,
            dividend.register_expr(),
            divisor.register_expr(),
            outcome.register_expr(),
        )?;

        Ok(ArithConfig {
            r_insn,
            dividend,
            divisor,
            outcome,
            remainder: r,
            inter_mul_value,
            is_zero,
            remainder_lt: lt,
        })
    }

    fn assign_instance(
        config: &Self::InstructionConfig,
        instance: &mut [MaybeUninit<E::BaseField>],
        lkm: &mut LkMultiplicity,
        step: &StepRecord,
    ) -> Result<(), ZKVMError> {
        let rs1 = step.rs1().unwrap().value;
        let rs2 = step.rs2().unwrap().value;
        let rd = step.rd().unwrap().value.after;

        // dividend = divisor * outcome + r
        let divisor = Value::new_unchecked(rs2);
        let outcome = Value::new(rd, lkm);

        let r = Value::new(if rs2 == 0 { 0 } else { rs1 % rs2 }, lkm);

        // assignment
        config.r_insn.assign_instance(instance, lkm, step)?;
        config
            .divisor
            .assign_limbs(instance, divisor.as_u16_limbs());
        config
            .outcome
            .assign_limbs(instance, outcome.as_u16_limbs());

        let (dividend, inter_mul_value) = divisor.mul_add(&outcome, &r, lkm, true);
        config
            .inter_mul_value
            .assign_mul_outcome(instance, lkm, &inter_mul_value)?;

        config.dividend.assign_add_outcome(instance, &dividend);
        config.remainder.assign_limbs(instance, r.as_u16_limbs());
        config
            .is_zero
            .assign_instance(instance, divisor.as_u64().into())?;
        config
            .remainder_lt
            .assign_instance(instance, lkm, r.as_u64(), divisor.as_u64())?;

        Ok(())
    }
}

#[cfg(test)]
mod test {

    mod divu {

        use ceno_emul::{Change, InsnKind, StepRecord, Word, encode_rv32};
        use goldilocks::GoldilocksExt2;
        use itertools::Itertools;
        use multilinear_extensions::mle::IntoMLEs;
        use rand::Rng;

        use crate::{
            Value,
            circuit_builder::{CircuitBuilder, ConstraintSystem},
            instructions::{
                Instruction,
                riscv::{constants::UInt, divu::DivUInstruction},
            },
            scheme::mock_prover::{MOCK_PC_START, MockProver},
        };

        fn verify(
            name: &'static str,
            dividend: Word,
            divisor: Word,
            exp_outcome: Word,
            is_ok: bool,
        ) {
            let mut cs = ConstraintSystem::<GoldilocksExt2>::new(|| "riscv");
            let mut cb = CircuitBuilder::new(&mut cs);
            let config = cb
                .namespace(
                    || format!("divu_({name})"),
                    |cb| Ok(DivUInstruction::construct_circuit(cb)),
                )
                .unwrap()
                .unwrap();

            let outcome = if divisor == 0 {
                u32::MAX
            } else {
                dividend / divisor
            };

            let insn_code = encode_rv32(InsnKind::DIVU, 2, 3, 4, 0);
            // values assignment
            let (raw_witin, lkm) =
                DivUInstruction::assign_instances(&config, cb.cs.num_witin as usize, vec![
                    StepRecord::new_r_instruction(
                        3,
                        MOCK_PC_START,
                        insn_code,
                        dividend,
                        divisor,
                        Change::new(0, outcome),
                        0,
                    ),
                ])
                .unwrap();

            let expected_rd_written = UInt::from_const_unchecked(
                Value::new_unchecked(exp_outcome).as_u16_limbs().to_vec(),
            );

            config
                .outcome
                .require_equal(|| "assert_outcome", &mut cb, &expected_rd_written)
                .unwrap();

            let expected_errors: &[_] = if is_ok { &[] } else { &[name] };
            MockProver::assert_with_expected_errors(
                &cb,
                &raw_witin
                    .de_interleaving()
                    .into_mles()
                    .into_iter()
                    .map(|v| v.into())
                    .collect_vec(),
                &[insn_code],
                expected_errors,
                None,
                Some(lkm),
            );
        }
        #[test]
        fn test_opcode_divu() {
            verify("basic", 10, 2, 5, true);
            verify("dividend > divisor", 10, 11, 0, true);
            verify("remainder", 11, 2, 5, true);
            verify("u32::MAX", u32::MAX, u32::MAX, 1, true);
            verify("div u32::MAX", 3, u32::MAX, 0, true);
            verify("u32::MAX div by 2", u32::MAX, 2, u32::MAX / 2, true);
            verify("mul with carries", 1202729773, 171818539, 7, true);
            verify("div by zero", 10, 0, u32::MAX, true);
        }

        #[test]
        fn test_opcode_divu_unstatisfied() {
            verify("assert_outcome", 10, 2, 3, false);
        }

        #[test]
        fn test_opcode_divu_random() {
            let mut rng = rand::thread_rng();
            let a: u32 = rng.gen();
            let b: u32 = rng.gen_range(1..u32::MAX);
            verify("random", a, b, a / b, true);
        }
    }
}
