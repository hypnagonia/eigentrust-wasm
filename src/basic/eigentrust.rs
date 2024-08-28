use crate::sparse::entry::Entry;
use crate::sparse::matrix::{CSMatrix, CSRMatrix};
use crate::sparse::vector::Vector;
use std::cmp;
use std::collections::HashMap;
use std::error::Error;
use std::f64;
use wasm_bindgen::prelude::*;

use web_sys::console;

// Canonicalize scales sparse entries in-place so that their values sum to one.
// If entries sum to zero, Canonicalize returns an error indicating a zero-sum vector.
pub fn canonicalize(entries: &mut [Entry]) -> Result<(), String> {
    let sum: f64 = entries.iter().map(|entry| entry.value).sum();
    if sum == 0.0 {
        return Err("Zero sum vector".to_string());
    }
    for entry in entries.iter_mut() {
        entry.value /= sum;
    }
    Ok(())
}

// ConvergenceChecker checks for convergence of trust vector series.
pub struct ConvergenceChecker {
    iter: usize,
    t: Vector,
    d: f64,
    e: f64,
}

impl ConvergenceChecker {
    // Creates a new convergence checker.
    pub fn new(t0: &Vector, e: f64) -> ConvergenceChecker {
        ConvergenceChecker {
            iter: 0,
            t: t0.clone(),
            d: 2.0 * e, // initial sentinel
            e,
        }
    }

    // Updates the checker with another iteration of the trust vector.
    pub fn update(&mut self, t: &Vector) -> Result<(), String> {
        let mut td = Vector::new(self.t.dim, vec![]);
        td.sub_vec(t, &self.t)?;
        let d = td.norm2();
        self.t.assign(t);
        self.d = d;
        self.iter += 1;
        Ok(())
    }

    // Returns true if the last updated vector has converged.
    pub fn converged(&self) -> bool {
        self.d <= self.e
    }

    // Returns the delta computed as of the last Update call.
    pub fn delta(&self) -> f64 {
        self.d
    }
}

// FlatTailChecker checks for a flat tail.
pub struct FlatTailChecker {
    length: usize,
    num_leaders: usize,
    stats: FlatTailStats,
}

impl FlatTailChecker {
    // Creates a new flat tail checker.
    pub fn new(length: usize, num_leaders: usize) -> FlatTailChecker {
        FlatTailChecker {
            length,
            num_leaders,
            stats: FlatTailStats {
                length: 0,
                threshold: 1,
                delta_norm: 1.0,
                ranking: vec![],
            },
        }
    }

    // Updates the checker with another iteration of the trust vector.
    pub fn update(&mut self, t: &Vector, d: f64) {
        let mut entries = t.entries.clone();
        entries.sort_by(|a, b| {
            b.value
                .partial_cmp(&a.value)
                .unwrap_or(cmp::Ordering::Equal)
        });
        let ranking: Vec<usize> = entries.iter().map(|entry| entry.index).collect();

        if ranking == self.stats.ranking {
            self.stats.length += 1;
        } else {
            if self.stats.length > 0 && self.stats.threshold <= self.stats.length {
                self.stats.threshold = self.stats.length + 1;
            }
            self.stats.length = 0;
            self.stats.delta_norm = d;
            self.stats.ranking = ranking;
        }
    }

    // Returns whether a flat tail has been reached.
    pub fn reached(&self) -> bool {
        self.stats.length >= self.length
    }
}

// FlatTailStats represents statistics about a flat tail.
pub struct FlatTailStats {
    pub length: usize,
    pub threshold: usize,
    pub delta_norm: f64,
    pub ranking: Vec<usize>,
}

#[cfg(target_arch = "wasm32")]
pub fn log_message(message: &str) {
    console::log_1(&message.into());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn log_message(message: &str) {
    println!("{}", message);
}

// Compute function implements the EigenTrust algorithm.
// todo Error instead of String
pub fn compute(
    c: &CSRMatrix,
    p: &Vector,
    a: f64,
    e: f64,
    max_iterations: Option<usize>,
    min_iterations: Option<usize>,
) -> Result<Vector, String> {
    let n = c.cs_matrix.major_dim;
    if n == 0 {
        return Err("Empty local trust matrix".to_string());
    }

    if p.dim != n {
        return Err("Dimension mismatch".to_string());
    }

    let mut t = p.clone();
    let mut t1 = p.clone();
    let ct = c.transpose()?;
    let mut ap = p.clone();
    ap.scale_vec(a, p);

    let mut conv_checker = ConvergenceChecker::new(&t, e);
    let mut flat_tail_checker = FlatTailChecker::new(min_iterations.unwrap_or(1), n);

    let mut iter = 0;
    let max_iters = max_iterations.unwrap_or(40); // usize::MAX
    let min_iters = min_iterations.unwrap_or(1);

    while iter < max_iters {
        // println!("iter {:?} {:?}\n", iter, flat_tail_checker.reached());

        // todo check freq
        // todo flat tail
        if iter >= min_iters
            && conv_checker.update(&t1).is_ok()
            && conv_checker.converged()
            && flat_tail_checker.reached()
        {
            break;
        }

        //
        if iter >= min_iters && conv_checker.converged() {
            // && flat_tail_checker.reached() {
            break;
        }

        let t1_clone = t1.clone();
        let mut new_t1 = t1.clone();
        new_t1.mul_vec(&ct, &t1_clone)?;
        let t2_clone = new_t1.clone();
        new_t1.scale_vec(1.0 - a, &t2_clone);
        t1.add_vec(&new_t1, &ap)?;

        // console::log_1(&"Hello from Rust!".into());
        let message = format!(
            "finished: dim = {}, nnz = {}, alpha = {}, epsilon = {}, iterations = {}",
            n,
            ct.cs_matrix.nnz(),
            a,
            e,
            iter
        );

        log_message(&message);

        iter += 1;
    }

    if iter >= max_iters {
        return Err("Reached maximum iterations without convergence".to_string());
    }

    Ok(t1)
}

pub fn discount_trust_vector(t: &mut Vector, discounts: &CSRMatrix) -> Result<(), String> {
    let mut i1 = 0;
    let t1 = t.clone();

    'DiscountsLoop: for (distruster, distrusts) in discounts.cs_matrix.entries.iter().enumerate() {
        'T1Loop: loop {
            if i1 >= t1.entries.len() {
                // No more nonzero trust, remaining distrusters have zero rep
                // and their distrusts do not matter, so finish

                break 'DiscountsLoop;
            }
            if t1.entries[i1].index < distruster {
                // The peer at i1 has no distrust, advance to the next peer
                i1 += 1;
                continue 'T1Loop;
            }
            if t1.entries[i1].index == distruster {
                // Found a match!
                break 'T1Loop;
            }
            if t1.entries[i1].index > distruster {
                // Distruster has zero rep, advance to the next distruster
                continue 'DiscountsLoop;
            }
        }

        let scaled_distrust_vec = {
            let mut temp_vec = Vector::new(t.dim, Vec::new());
            temp_vec.scale_vec(
                t1.entries[i1].value,
                &(Vector {
                    dim: t.dim,
                    entries: distrusts.clone(),
                }),
            );
            temp_vec
        };

        let t2 = t.clone();
        t.sub_vec(&t2, &scaled_distrust_vec)?;

        i1 += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sparse::entry::Entry;
    use crate::sparse::matrix::CSRMatrix;
    use crate::sparse::vector::Vector;

    #[test]
    fn test_discount_trust_vector() {
        struct TestCase {
            name: &'static str,
            t: Vector,
            discounts: CSRMatrix,
            expected: Vector,
        }

        let test_cases = vec![TestCase {
            name: "test1",
            t: Vector::new(
                5,
                vec![
                    Entry {
                        index: 0,
                        value: 0.25,
                    },
                    Entry {
                        index: 2,
                        value: 0.5,
                    },
                    Entry {
                        index: 3,
                        value: 0.25,
                    },
                ],
            ),
            discounts: CSRMatrix {
                cs_matrix: CSMatrix {
                    major_dim: 5,
                    minor_dim: 5,
                    entries: vec![
                        // 0 - no distrust (empty)
                        vec![],
                        // 1 - doesn't matter because of zero trust
                        vec![
                            Entry {
                                index: 2,
                                value: 0.5,
                            },
                            Entry {
                                index: 3,
                                value: 0.5,
                            },
                        ],
                        // 2 - scaled by 0.5 and applied
                        vec![
                            Entry {
                                index: 0,
                                value: 0.25,
                            },
                            Entry {
                                index: 4,
                                value: 0.75,
                            },
                        ],
                        // 3 - scaled by 0.25 and applied
                        vec![
                            Entry {
                                index: 2,
                                value: 0.5,
                            },
                            Entry {
                                index: 4,
                                value: 0.5,
                            },
                        ],
                        // 4 - no distrust, also zero global trust (empty)
                        vec![],
                    ],
                },
            },
            expected: Vector::new(
                5,
                vec![
                    // {index, original - distrust*gt}
                    Entry {
                        index: 0,
                        value: 0.25 - 0.25 * 0.5,
                    }, // peer 2
                    Entry {
                        index: 2,
                        value: 0.5 - 0.5 * 0.25,
                    }, // peer 3
                    Entry {
                        index: 3,
                        value: 0.25,
                    },
                    Entry {
                        index: 4,
                        value: 0.0 - 0.75 * 0.5 - 0.5 * 0.25,
                    }, // peer 2 & 3
                ],
            ),
        }];

        for test in test_cases {
            let mut t = test.t.clone();
            let result = discount_trust_vector(&mut t, &test.discounts);
            assert!(result.is_ok(), "{}: DiscountTrustVector failed", test.name);
            assert_eq!(
                t, test.expected,
                "{}: Vector does not match expected value",
                test.name
            );
        }
    }

    #[test]
    fn test_run() {
        let e = 1.25e-7;
        let a = 0.5;

        let p = Vector::new(
            8,
            vec![
                Entry {
                    index: 0,
                    value: 0.14285714285714285,
                },
                Entry {
                    index: 1,
                    value: 0.14285714285714285,
                },
                Entry {
                    index: 2,
                    value: 0.14285714285714285,
                },
                Entry {
                    index: 3,
                    value: 0.14285714285714285,
                },
                Entry {
                    index: 4,
                    value: 0.14285714285714285,
                },
                Entry {
                    index: 5,
                    value: 0.14285714285714285,
                },
                Entry {
                    index: 6,
                    value: 0.14285714285714285,
                },
            ],
        );

        let c = CSRMatrix {
            cs_matrix: CSMatrix {
                major_dim: 8,
                minor_dim: 8,
                entries: vec![
                    vec![Entry {
                        index: 3,
                        value: 1.0,
                    }],
                    vec![
                        Entry {
                            index: 0,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 1,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 2,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 3,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 4,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 5,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 6,
                            value: 0.14285714285714285,
                        },
                    ],
                    vec![Entry {
                        index: 3,
                        value: 1.0,
                    }],
                    vec![
                        Entry {
                            index: 0,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 1,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 2,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 3,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 4,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 5,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 6,
                            value: 0.14285714285714285,
                        },
                    ],
                    vec![Entry {
                        index: 1,
                        value: 1.0,
                    }],
                    vec![
                        Entry {
                            index: 0,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 1,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 2,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 3,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 4,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 5,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 6,
                            value: 0.14285714285714285,
                        },
                    ],
                    vec![Entry {
                        index: 5,
                        value: 1.0,
                    }],
                    vec![
                        Entry {
                            index: 0,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 1,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 2,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 3,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 4,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 5,
                            value: 0.14285714285714285,
                        },
                        Entry {
                            index: 6,
                            value: 0.14285714285714285,
                        },
                    ],
                ],
            },
        };

        let expected = Vector {
            dim: 8,
            entries: vec![
                Entry {
                    index: 0,
                    value: 0.11111110842697292,
                },
                Entry {
                    index: 1,
                    value: 0.16666666867977029,
                },
                Entry {
                    index: 2,
                    value: 0.11111110842697292,
                },
                Entry {
                    index: 3,
                    value: 0.22222222893256766,
                },
                Entry {
                    index: 4,
                    value: 0.11111110842697292,
                },
                Entry {
                    index: 5,
                    value: 0.16666666867977029,
                },
                Entry {
                    index: 6,
                    value: 0.11111110842697292,
                },
            ],
        };
        let result = compute(&c, &p, a, e, None, None).unwrap();
        assert_eq!(result, expected);
    }
}
