use super::utils::{eval_by_expr, wit_infer_by_expr};
use crate::{circuit_builder::CircuitBuilder, expression::Expression, structs::ROMType};
use ff_ext::ExtensionField;
use itertools::Itertools;
use multilinear_extensions::virtual_poly_v2::ArcMultilinearExtension;
use std::{marker::PhantomData, ops::Neg};

#[allow(clippy::enum_variant_names)]
#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone)]
enum MockProverError<E: ExtensionField> {
    AssertZeroError {
        expression: Expression<E>,
        evaluated: E,
        name: String,
    },
    AssertEqualError {
        left_expression: Expression<E>,
        right_expression: Expression<E>,
        left: E,
        right: E,
        name: String,
    },
    LookupError {
        expression: Expression<E>,
        evaluated: E,
        name: String,
    },
    // TODO later
    // r_expressions
    // w_expressions
}

impl<E: ExtensionField> MockProverError<E> {
    #[allow(dead_code)]
    pub fn print(&self) {
        match self {
            Self::AssertZeroError {
                expression,
                evaluated,
                name,
            } => {
                println!(
                    "\nAssertZeroError {name:#?}: Evaluated expression is not zero\nExpression: \
                    {expression:?}\nEvaluation: {evaluated:?}\n",
                );
            }
            Self::AssertEqualError {
                left_expression,
                right_expression,
                left,
                right,
                name,
            } => {
                println!(
                    "\nAssertEqualError {name:#?}: Left != Right\n\
                    Left: {left:?}\nRight: {right:?}\n\
                    Left Expression: {left_expression:?}\n\
                    Right Expression: {right_expression:?}\n",
                );
            }
            Self::LookupError {
                expression,
                evaluated,
                name,
            } => {
                println!(
                    "\nLookupError {name:#?}: Evaluated expression does not exist in T \
                    vector\nExpression: {expression:?}\nEvaluation: {evaluated:?}\n",
                );
            }
        }
    }
}

#[allow(dead_code)] // TODO remove
struct MockProver<E: ExtensionField> {
    _phantom: PhantomData<E>,
}

impl<'a, E: ExtensionField> MockProver<E> {
    #[allow(dead_code)]
    pub fn run(
        cb: &mut CircuitBuilder<E>,
        wits_in: &[ArcMultilinearExtension<'a, E>],
        challenge: Option<[E; 2]>,
    ) -> Result<(), Vec<MockProverError<E>>> {
        let challenge = challenge.unwrap_or([E::ONE, E::ONE]);

        let mut errors = vec![];

        // Assert zero expressions
        for (expr, name) in cb
            .cs
            .assert_zero_expressions
            .iter()
            .chain(&cb.cs.assert_zero_sumcheck_expressions)
            .zip_eq(
                cb.cs
                    .assert_zero_expressions_namespace_map
                    .iter()
                    .chain(&cb.cs.assert_zero_sumcheck_expressions_namespace_map),
            )
        {
            if name.contains("require_equal") {
                let (left, right) = expr.unpack_sum().unwrap();

                let left = left.neg().neg(); // TODO get_ext_field_vec doesn't work without this
                let right = right.neg();

                let left_evaluated = wit_infer_by_expr(wits_in, &challenge, &left);
                let left_evaluated = left_evaluated.get_ext_field_vec();

                let right_evaluated = wit_infer_by_expr(wits_in, &challenge, &right);
                let right_evaluated = right_evaluated.get_ext_field_vec();

                for (left_element, right_element) in left_evaluated.iter().zip_eq(right_evaluated) {
                    if *left_element != *right_element {
                        errors.push(MockProverError::AssertEqualError {
                            left_expression: left.clone(),
                            right_expression: right.clone(),
                            left: *left_element,
                            right: *right_element,
                            name: name.clone(),
                        });
                    }
                }
            } else {
                let expr = expr.clone().neg().neg(); // TODO get_ext_field_vec doesn't work without this
                let expr_evaluated = wit_infer_by_expr(wits_in, &challenge, &expr);
                let expr_evaluated = expr_evaluated.get_ext_field_vec();

                for element in expr_evaluated {
                    if *element != E::ZERO {
                        errors.push(MockProverError::AssertZeroError {
                            expression: expr.clone(),
                            evaluated: *element,
                            name: name.clone(),
                        });
                    }
                }
            }
        }

        // TODO load more tables here
        let mut table_vec = vec![];
        load_u5_table(&mut table_vec, cb, challenge);

        // Lookup expressions
        for (expr, name) in cb
            .cs
            .lk_expressions
            .iter()
            .zip_eq(cb.cs.lk_expressions_namespace_map.iter())
        {
            let expr_evaluated = wit_infer_by_expr(wits_in, &challenge, expr);
            let expr_evaluated = expr_evaluated.get_ext_field_vec();

            // Check each lookup expr exists in t vec
            for element in expr_evaluated {
                if !table_vec.contains(element) {
                    errors.push(MockProverError::LookupError {
                        expression: expr.clone(),
                        evaluated: *element,
                        name: name.clone(),
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    #[allow(dead_code)]
    pub fn assert_satisfied(
        cb: &mut CircuitBuilder<E>,
        wits_in: &[ArcMultilinearExtension<'a, E>],
        challenge: Option<[E; 2]>,
    ) {
        let result = Self::run(cb, wits_in, challenge);
        match result {
            Ok(_) => {}
            Err(errors) => {
                for error in errors {
                    error.print();
                }
                panic!("Constraints not satisfied");
            }
        }
    }
}

pub fn load_u5_table<E: ExtensionField>(
    t_vec: &mut Vec<E>,
    cb: &CircuitBuilder<E>,
    challenge: [E; 2],
) {
    for i in 0..32 {
        let rlc_record = cb.rlc_chip_record(vec![
            Expression::Constant(E::BaseField::from(ROMType::U5 as u64)),
            i.into(),
        ]);
        let rlc_record = eval_by_expr(&[], &challenge, &rlc_record);
        t_vec.push(rlc_record);
    }
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        circuit_builder::{CircuitBuilder, ConstraintSystem},
        error::ZKVMError,
        expression::{ToExpr, WitIn},
    };
    use ff::Field;
    use goldilocks::{Goldilocks, GoldilocksExt2};
    use multilinear_extensions::mle::IntoMLE;

    #[derive(Debug)]
    #[allow(dead_code)]
    struct AssertZeroCircuit {
        pub a: WitIn,
        pub b: WitIn,
        pub c: WitIn,
    }

    impl AssertZeroCircuit {
        pub fn construct_circuit(
            cb: &mut CircuitBuilder<GoldilocksExt2>,
        ) -> Result<Self, ZKVMError> {
            let a = cb.create_witin(|| "a")?;
            let b = cb.create_witin(|| "b")?;
            let c = cb.create_witin(|| "c")?;

            // degree 1
            cb.require_equal(|| "a + 1 == b", b.expr(), a.expr() + 1.into())?;
            cb.require_zero(|| "c - 2 == 0", c.expr() - 2.into())?;

            // degree > 1
            let d = cb.create_witin(|| "d")?;
            cb.require_zero(
                || "d*d - 6*d + 9 == 0",
                d.expr() * d.expr() - d.expr() * 6.into() + 9.into(),
            )?;

            Ok(Self { a, b, c })
        }
    }

    #[test]
    fn test_assert_zero_1() {
        let mut cs = ConstraintSystem::new(|| "test_assert_zero_1");
        let mut builder = CircuitBuilder::<GoldilocksExt2>::new(&mut cs);

        let _ = AssertZeroCircuit::construct_circuit(&mut builder).unwrap();

        let wits_in = vec![
            vec![Goldilocks::from(3), Goldilocks::from(500)]
                .into_mle()
                .into(),
            vec![Goldilocks::from(4), Goldilocks::from(501)]
                .into_mle()
                .into(),
            vec![Goldilocks::from(2), Goldilocks::from(2)]
                .into_mle()
                .into(),
            vec![Goldilocks::from(3), Goldilocks::from(3)]
                .into_mle()
                .into(),
        ];

        MockProver::assert_satisfied(&mut builder, &wits_in, None);
    }

    #[derive(Debug)]
    struct RangeCheckCircuit {
        #[allow(dead_code)]
        pub a: WitIn,
    }

    impl RangeCheckCircuit {
        pub fn construct_circuit(
            cb: &mut CircuitBuilder<GoldilocksExt2>,
        ) -> Result<Self, ZKVMError> {
            let a = cb.create_witin(|| "a")?;
            cb.assert_ux::<_, _, 5>(|| "assert u5", a.expr())?;
            Ok(Self { a })
        }
    }

    #[test]
    fn test_lookup_1() {
        let mut cs = ConstraintSystem::new(|| "test_lookup_1");
        let mut builder = CircuitBuilder::<GoldilocksExt2>::new(&mut cs);

        let _ = RangeCheckCircuit::construct_circuit(&mut builder).unwrap();

        let wits_in = vec![
            vec![Goldilocks::from(3u64), Goldilocks::from(5u64)]
                .into_mle()
                .into(),
        ];

        let challenge = [1.into(), 1000.into()];
        MockProver::assert_satisfied(&mut builder, &wits_in, Some(challenge));
    }

    #[test]
    fn test_lookup_error() {
        let mut cs = ConstraintSystem::new(|| "test_lookup_error");
        let mut builder = CircuitBuilder::<GoldilocksExt2>::new(&mut cs);

        let _ = RangeCheckCircuit::construct_circuit(&mut builder).unwrap();

        let wits_in = vec![vec![Goldilocks::from(123)].into_mle().into()];

        let challenge = [2.into(), 1000.into()];
        let result = MockProver::run(&mut builder, &wits_in, Some(challenge));
        assert!(result.is_err(), "Expected error");
        let err = result.unwrap_err();
        assert_eq!(
            err,
            vec![MockProverError::LookupError {
                expression: Expression::ScaledSum(
                    Box::new(Expression::WitIn(0)),
                    Box::new(Expression::Challenge(
                        1,
                        1,
                        // TODO this still uses default challenge in ConstraintSystem, but challengeId
                        // helps to evaluate the expression correctly. Shoudl challenge be just challengeId?
                        GoldilocksExt2::ONE,
                        GoldilocksExt2::ZERO,
                    )),
                    Box::new(Expression::Challenge(
                        0,
                        1,
                        GoldilocksExt2::ONE,
                        GoldilocksExt2::ZERO,
                    )),
                ),
                evaluated: 123002.into(), // 123 * 1000 + 2
                name: "test_lookup_error/assert_u5/assert u5".to_string(),
            }]
        );
    }
}