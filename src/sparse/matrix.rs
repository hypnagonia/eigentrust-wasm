use std::cmp::Ordering;
use std::ptr;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;

use super::entry::Entry;
use super::vector::Vector;

// Compressed Sparse Matrix (CSMatrix) equivalent in Rust
#[derive(Clone, PartialEq, Debug)]
pub struct CSMatrix {
    pub major_dim: usize,
    pub minor_dim: usize,
    pub entries: Vec<Vec<Entry>>,
}

impl CSMatrix {
    pub fn new() -> Self {
        Self {
            major_dim: 0,
            minor_dim: 0,
            entries: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.major_dim = 0;
        self.minor_dim = 0;
        self.entries.clear();
    }

    pub fn dim(&self) -> Result<usize, &'static str> {
        if self.major_dim != self.minor_dim {
            return Err("Dimension mismatch");
        }
        Ok(self.major_dim)
    }

    pub fn set_major_dim(&mut self, dim: usize) {
        if self.entries.capacity() < dim {
            let mut new_entries = Vec::with_capacity(dim);
            new_entries.extend(self.entries.drain(..));
            self.entries = new_entries;
        }
        self.entries.resize_with(dim, Vec::new);
        self.major_dim = dim;
    }

    pub fn set_minor_dim(&mut self, dim: usize) {
        for entries in &mut self.entries {
            entries.retain(|e| e.index < dim);
        }
        self.minor_dim = dim;
    }

    pub fn nnz(&self) -> usize {
        self.entries.iter().map(|row| row.len()).sum()
    }

    pub fn transpose(&self) -> Result<CSMatrix, JsValue> {
        let mut nnzs = vec![0; self.minor_dim];
        for row_entries in &self.entries {
            for entry in row_entries {
                nnzs[entry.index] += 1;
            }
        }

        let mut transposed_entries = vec![Vec::new(); self.minor_dim];
        for (col, &nnz) in nnzs.iter().enumerate() {
            if nnz != 0 {
                transposed_entries[col].reserve(nnz);
            }
        }

        for (row, row_entries) in self.entries.iter().enumerate() {
            for entry in row_entries {
                transposed_entries[entry.index].push(Entry {
                    index: row,
                    value: entry.value,
                });
            }
        }

        Ok(CSMatrix {
            major_dim: self.minor_dim,
            minor_dim: self.major_dim,
            entries: transposed_entries,
        })
    }

    pub fn merge(&mut self, other: &mut CSMatrix) {
        self.set_major_dim(self.major_dim.max(other.major_dim));
        self.set_minor_dim(self.minor_dim.max(other.minor_dim));
        for i in 0..other.major_dim {
            self.entries[i] = merge_span(&self.entries[i], &other.entries[i]);
        }
        other.reset();
    }
}

// Merges two spans
fn merge_span(s1: &[Entry], s2: &[Entry]) -> Vec<Entry> {
    let mut s = Vec::with_capacity(s1.len() + s2.len());
    let mut i1 = 0;
    let mut i2 = 0;

    while i1 < s1.len() || i2 < s2.len() {
        if i2 >= s2.len() {
            s.push(s1[i1].clone());
            i1 += 1;
        } else if i1 >= s1.len() {
            s.push(s2[i2].clone());
            i2 += 1;
        } else if s1[i1].index < s2[i2].index {
            s.push(s1[i1].clone());
            i1 += 1;
        } else if s1[i1].index > s2[i2].index {
            s.push(s2[i2].clone());
            i2 += 1;
        } else {
            s.push(s2[i2].clone());
            i1 += 1;
            i2 += 1;
        }
    }

    s.shrink_to_fit();
    s
}

// CSRMatrix implementation
#[derive(Clone, PartialEq, Debug)]
pub struct CSRMatrix {
    pub cs_matrix: CSMatrix,
}

impl CSRMatrix {
    pub fn new(rows: usize, cols: usize, entries: Vec<(usize, usize, f64)>) -> Self {
        let mut matrix_entries = vec![Vec::new(); rows];

        for (row, col, value) in entries {
            if value != 0.0 {
                matrix_entries[row].push(Entry { index: col, value });
            }
        }

        for row in &mut matrix_entries {
            row.sort_by(|a, b| a.index.cmp(&b.index));
        }

        CSRMatrix {
            cs_matrix: CSMatrix {
                major_dim: rows,
                minor_dim: cols,
                entries: matrix_entries,
            },
        }
    }

    pub fn dims(&self) -> (usize, usize) {
        (self.cs_matrix.major_dim, self.cs_matrix.minor_dim)
    }

    pub fn set_dim(&mut self, rows: usize, cols: usize) {
        self.cs_matrix.set_major_dim(rows);
        self.cs_matrix.set_minor_dim(cols);
    }

    pub fn row_vector(&self, index: usize) -> Vector {
        Vector {
            dim: self.cs_matrix.minor_dim,
            entries: self.cs_matrix.entries[index].clone(),
        }
    }

    pub fn set_row_vector(&mut self, index: usize, vector: Vector) {
        self.cs_matrix.entries[index] = vector.entries;
    }

    pub fn transpose(&self) -> Result<CSRMatrix, JsValue> {
        let transposed = self.cs_matrix.transpose()?;
        Ok(CSRMatrix { cs_matrix: transposed })
    }

    pub fn transpose_to_csc(&self) -> CSCMatrix {
        CSCMatrix {
            cs_matrix: CSMatrix {
                major_dim: self.cs_matrix.minor_dim,
                minor_dim: self.cs_matrix.major_dim,
                entries: self.cs_matrix.entries.clone(),
            },
        }
    }
}

// CSCMatrix implementation
#[derive(Clone, PartialEq, Debug)]
pub struct CSCMatrix {
    pub cs_matrix: CSMatrix,
}

impl CSCMatrix {
    pub fn dims(&self) -> (usize, usize) {
        (self.cs_matrix.minor_dim, self.cs_matrix.major_dim)
    }

    pub fn set_dim(&mut self, rows: usize, cols: usize) {
        self.cs_matrix.set_major_dim(cols);
        self.cs_matrix.set_minor_dim(rows);
    }

    pub fn column_vector(&self, index: usize) -> Vector {
        Vector {
            dim: self.cs_matrix.minor_dim,
            entries: self.cs_matrix.entries[index].clone(),
        }
    }

    pub fn transpose(&self) -> Result<CSCMatrix, JsValue> {
        let transposed = self.cs_matrix.transpose()?;
        Ok(CSCMatrix { cs_matrix: transposed })
    }

    pub fn transpose_to_csr(&self) -> CSRMatrix {
        CSRMatrix {
            cs_matrix: CSMatrix {
                major_dim: self.cs_matrix.minor_dim,
                minor_dim: self.cs_matrix.major_dim,
                entries: self.cs_matrix.entries.clone(),
            },
        }
    }
}

// Helper functions for creating and transposing matrices
// todo sparse Entry instead of Vec
pub fn create_csr_matrix(rows: usize, cols: usize, entries: Vec<(usize, usize, f64)>) -> CSRMatrix {
    CSRMatrix::new(rows, cols, entries)
}

pub fn transpose_csr_matrix(matrix: &CSRMatrix) -> Result<CSRMatrix, JsValue> {
    matrix.transpose()
}

pub fn transpose_to_csc(matrix: &CSRMatrix) -> CSCMatrix {
    matrix.transpose_to_csc()
}

