pub mod format;

use std::collections::BTreeMap;

use ciborium::Value as CborValue;
use cino_vm::VmValue;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CodecError {
    #[error("failed to decode CBOR: {0}")]
    Decode(#[from] ciborium::de::Error<std::io::Error>),
    #[error("failed to encode CBOR: {0}")]
    Encode(#[from] ciborium::ser::Error<std::io::Error>),
    #[error("invalid CBOR format for VmValue: {0}")]
    InvalidFormat(String),
}

/// Decode a canonical CBOR byte slice into a `VmValue`.
pub fn decode_value(bytes: &[u8]) -> Result<VmValue, CodecError> {
    let cbor: CborValue = ciborium::from_reader(bytes)?;
    vm_value_from_cbor(&cbor)
}

/// Encode `VmValue` into a canonical CBOR byte vector.
pub fn encode_value(value: &VmValue) -> Result<Vec<u8>, CodecError> {
    let mut bytes = Vec::new();
    let cbor = cbor_from_vm_value(value);
    ciborium::into_writer(&cbor, &mut bytes)?;
    Ok(bytes)
}

fn vm_value_from_cbor(value: &CborValue) -> Result<VmValue, CodecError> {
    match value {
        CborValue::Null => Ok(VmValue::Unit),
        CborValue::Bool(v) => Ok(VmValue::Bool(*v)),
        CborValue::Integer(v) => {
            let n: i128 = (*v).into();
            i64::try_from(n)
                .map(VmValue::Int)
                .map_err(|_| CodecError::InvalidFormat("integer does not fit in i64".to_string()))
        }
        CborValue::Text(s) => Ok(VmValue::String(s.clone())),
        CborValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(vm_value_from_cbor(item)?);
            }
            Ok(VmValue::List(out))
        }
        CborValue::Map(entries) => parse_vm_map(entries),
        _ => Err(CodecError::InvalidFormat(
            "unsupported CBOR type for VmValue".to_string(),
        )),
    }
}

fn parse_vm_map(entries: &[(CborValue, CborValue)]) -> Result<VmValue, CodecError> {
    if entries.len() == 1 {
        let (k, v) = &entries[0];
        if k == &CborValue::Text("$tuple".to_string()) {
            let CborValue::Array(items) = v else {
                return Err(CodecError::InvalidFormat(
                    "`$tuple` must be an array".to_string(),
                ));
            };
            let mut tuple = Vec::with_capacity(items.len());
            for item in items {
                tuple.push(vm_value_from_cbor(item)?);
            }
            return Ok(VmValue::Tuple(tuple));
        }
    }

    let mut map = BTreeMap::new();
    let mut tag = None;
    let mut fields = None;

    for (k, v) in entries {
        let CborValue::Text(k_str) = k else {
            return Err(CodecError::InvalidFormat(
                "map key must be text".to_string(),
            ));
        };

        if k_str == "$tag" {
            let CborValue::Text(t) = v else {
                return Err(CodecError::InvalidFormat("`$tag` must be text".to_string()));
            };
            tag = Some(t.clone());
        } else if k_str == "$fields" {
            let CborValue::Map(f) = v else {
                return Err(CodecError::InvalidFormat(
                    "`$fields` must be a map".to_string(),
                ));
            };

            let mut parsed_fields = BTreeMap::new();
            for (fk, fv) in f {
                let CborValue::Text(fk_str) = fk else {
                    return Err(CodecError::InvalidFormat(
                        "enum field key must be text".to_string(),
                    ));
                };
                parsed_fields.insert(fk_str.clone(), vm_value_from_cbor(fv)?);
            }
            fields = Some(parsed_fields);
        } else {
            map.insert(k_str.clone(), vm_value_from_cbor(v)?);
        }
    }

    if tag.is_some() || fields.is_some() {
        let tag =
            tag.ok_or_else(|| CodecError::InvalidFormat("enum map is missing `$tag`".to_string()))?;
        let fields = fields.ok_or_else(|| {
            CodecError::InvalidFormat("enum map is missing `$fields`".to_string())
        })?;
        Ok(VmValue::Enum { tag, fields })
    } else {
        Ok(VmValue::Map(map))
    }
}

fn cbor_from_vm_value(value: &VmValue) -> CborValue {
    match value {
        VmValue::Unit => CborValue::Null,
        VmValue::Int(v) => CborValue::Integer((*v).into()),
        VmValue::Bool(v) => CborValue::Bool(*v),
        VmValue::String(v) => CborValue::Text(v.clone()),
        VmValue::List(items) => CborValue::Array(items.iter().map(cbor_from_vm_value).collect()),
        VmValue::Tuple(items) => CborValue::Map(vec![(
            CborValue::Text("$tuple".to_string()),
            CborValue::Array(items.iter().map(cbor_from_vm_value).collect()),
        )]),
        VmValue::Map(entries) => {
            // ciborium encodes maps in order, so we need to ensure lexical ordering of keys when encoded
            // But ciborium's serializers already sort maps canonically by default if we use into_writer.
            // BTreeMap naturally iterates keys in order, which also matches the requirement.
            let mut map = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                map.push((CborValue::Text(key.clone()), cbor_from_vm_value(value)));
            }
            CborValue::Map(map)
        }
        VmValue::Enum { tag, fields } => {
            let mut raw_fields = Vec::with_capacity(fields.len());
            for (key, value) in fields {
                raw_fields.push((CborValue::Text(key.clone()), cbor_from_vm_value(value)));
            }
            CborValue::Map(vec![
                (
                    CborValue::Text("$fields".to_string()),
                    CborValue::Map(raw_fields),
                ),
                (
                    CborValue::Text("$tag".to_string()),
                    CborValue::Text(tag.clone()),
                ),
            ])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbor_roundtrip_for_tuple_and_enum() {
        let value = VmValue::Tuple(vec![
            VmValue::Int(1),
            VmValue::Enum {
                tag: "Event".to_string(),
                fields: BTreeMap::from([("id".to_string(), VmValue::Int(42))]),
            },
        ]);

        let encoded = encode_value(&value).expect("must encode");
        let decoded = decode_value(&encoded).expect("must decode");
        assert_eq!(decoded, value);
    }
}
