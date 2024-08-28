use crate::sparse::entry::Entry;
use crate::sparse::vector::Vector;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

// CanonicalizeTrustVector canonicalizes the trust vector in-place,
// scaling it so that the elements sum to one,
// or making it a uniform vector that sums to one if it's a zero vector.
pub fn canonicalize_trust_vector(v: &mut Vec<Entry>) {
    if canonicalize(v).is_err() {
        let dim = v.len();
        let c = 1.0 / dim as f64;
        v.clear();
        for i in 0..dim {
            v.push(Entry { index: i, value: c });
        }
    }
}

// Helper function to canonicalize a vector in-place.
// Returns an error if the vector is a zero vector.
fn canonicalize(entries: &mut Vec<Entry>) -> Result<(), &'static str> {
    let sum: f64 = entries.iter().map(|entry| entry.value).sum();

    if sum == 0.0 {
        return Err("Zero sum vector");
    }

    for entry in entries.iter_mut() {
        entry.value /= sum;
    }

    Ok(())
}

pub fn read_trust_vector_from_csv(
    input: &str,
    peer_indices: &HashMap<String, usize>,
) -> Result<Vector, String> {
    let mut count = 0;
    let mut max_peer = -1;
    let mut entries = Vec::new();

    for line in input.lines() {
        count += 1;
        let fields: Vec<&str> = line.split(',').collect();

        let (peer, level) = match fields.len() {
            0 => return Err(format!("too few fields in line {}", count)),
            _ => {
                let peer = parse_peer_id(fields[0], peer_indices).map_err(|e| {
                    format!("invalid peer {:?} in line {}: {}", fields[0], count, e)
                })?;
                let level = if fields.len() >= 2 {
                    parse_trust_level(fields[1]).map_err(|e| {
                        format!(
                            "invalid trust level {:?} in line {}: {}",
                            fields[1], count, e
                        )
                    })?
                } else {
                    1.0
                };
                (peer, level)
            }
        };

        if max_peer < peer as isize {
            max_peer = peer as isize;
        }

        entries.push(Entry {
            index: peer,
            value: level,
        });
    }

    Ok(Vector::new((max_peer + 1) as usize, entries))
}

fn parse_peer_id(peer_str: &str, peer_indices: &HashMap<String, usize>) -> Result<usize, String> {
    peer_indices
        .get(peer_str)
        .cloned()
        .ok_or_else(|| format!("Invalid peer: {}", peer_str))
}

fn parse_trust_level(level_str: &str) -> Result<f64, String> {
    level_str
        .parse::<f64>()
        .map_err(|_| format!("Invalid trust level: {}", level_str))
}
