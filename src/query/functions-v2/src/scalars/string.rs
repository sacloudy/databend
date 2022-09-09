// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cmp::Ordering;
use std::io::Write;

use bstr::ByteSlice;
use common_expression::types::number::NumberDomain;
use common_expression::types::number::UInt64Type;
use common_expression::types::string::StringColumn;
use common_expression::types::string::StringColumnBuilder;
use common_expression::types::GenericMap;
use common_expression::types::NumberType;
use common_expression::types::StringType;
use common_expression::vectorize_with_builder_1_arg;
use common_expression::FunctionProperty;
use common_expression::FunctionRegistry;
use common_expression::Value;
use common_expression::ValueRef;
use itertools::izip;

use super::soundex::Soundex;

pub fn register(registry: &mut FunctionRegistry) {
    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "upper",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len(),
            |val, writer| {
                for (start, end, ch) in val.char_indices() {
                    if ch == '\u{FFFD}' {
                        // If char is invalid, just copy it.
                        writer.put_slice(&val.as_bytes()[start..end]);
                    } else if ch.is_ascii() {
                        writer.put_u8(ch.to_ascii_uppercase() as u8);
                    } else {
                        for x in ch.to_uppercase() {
                            writer.put_char(x);
                        }
                    }
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );
    registry.register_aliases("upper", &["ucase"]);

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "lower",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len(),
            |val, writer| {
                for (start, end, ch) in val.char_indices() {
                    if ch == '\u{FFFD}' {
                        // If char is invalid, just copy it.
                        writer.put_slice(&val.as_bytes()[start..end]);
                    } else if ch.is_ascii() {
                        writer.put_u8(ch.to_ascii_lowercase() as u8);
                    } else {
                        for x in ch.to_lowercase() {
                            writer.put_char(x);
                        }
                    }
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );
    registry.register_aliases("lower", &["lcase"]);

    registry.register_1_arg::<StringType, NumberType<u64>, _, _>(
        "bit_length",
        FunctionProperty::default(),
        |_| None,
        |val| 8 * val.len() as u64,
    );

    registry.register_1_arg::<StringType, NumberType<u64>, _, _>(
        "octet_length",
        FunctionProperty::default(),
        |_| None,
        |val| val.len() as u64,
    );
    registry.register_aliases("octet_length", &["length"]);

    registry.register_1_arg::<StringType, NumberType<u64>, _, _>(
        "char_length",
        FunctionProperty::default(),
        |_| None,
        |val| match std::str::from_utf8(val) {
            Ok(s) => s.chars().count() as u64,
            Err(_) => val.len() as u64,
        },
    );
    registry.register_aliases("char_length", &["character_length"]);

    registry.register_3_arg::<StringType, NumberType<u64>, StringType, StringType, _, _>(
        "lpad",
        FunctionProperty::default(),
        |_, _, _| None,
        |str: &[u8], l: u64, pad: &[u8]| {
            let mut buff: Vec<u8> = vec![];
            if l != 0 {
                if l > str.len() as u64 {
                    let l = l - str.len() as u64;
                    while buff.len() < l as usize {
                        if buff.len() + pad.len() <= l as usize {
                            buff.extend_from_slice(pad);
                        } else {
                            buff.extend_from_slice(&pad[0..l as usize - buff.len()])
                        }
                    }
                    buff.extend_from_slice(str);
                } else {
                    buff.extend_from_slice(&str[0..l as usize]);
                }
            }
            buff
        },
    );

    registry.register_3_arg::<StringType, NumberType<u64>, StringType, StringType, _, _>(
        "rpad",
        FunctionProperty::default(),
        |_, _, _| None,
        |str: &[u8], l: u64, pad: &[u8]| {
            let mut buff: Vec<u8> = vec![];
            if l != 0 {
                if l > str.len() as u64 {
                    buff.extend_from_slice(str);
                    while buff.len() < l as usize {
                        if buff.len() + pad.len() <= l as usize {
                            buff.extend_from_slice(pad);
                        } else {
                            buff.extend_from_slice(&pad[0..l as usize - buff.len()])
                        }
                    }
                } else {
                    buff.extend_from_slice(&str[0..l as usize]);
                }
            }
            buff
        },
    );

    registry.register_3_arg::<StringType, StringType, StringType, StringType, _, _>(
        "replace",
        FunctionProperty::default(),
        |_, _, _| None,
        |str, from, to| {
            let mut buf: Vec<u8> = vec![];
            if from.is_empty() || from == to {
                buf.extend_from_slice(str);
                return buf;
            }
            let mut skip = 0;
            for (p, w) in str.windows(from.len()).enumerate() {
                if w == from {
                    buf.extend_from_slice(to);
                    skip = from.len();
                } else if p + w.len() == str.len() {
                    buf.extend_from_slice(w);
                } else if skip > 1 {
                    skip -= 1;
                } else {
                    buf.extend_from_slice(&w[0..1]);
                }
            }
            buf
        },
    );

    registry.register_2_arg::<StringType, StringType, NumberType<i8>, _, _>(
        "strcmp",
        FunctionProperty::default(),
        |_, _| None,
        |s1, s2| {
            let res = match s1.len().cmp(&s2.len()) {
                Ordering::Equal => {
                    let mut res = Ordering::Equal;
                    for (s1i, s2i) in izip!(s1, s2) {
                        match s1i.cmp(s2i) {
                            Ordering::Equal => continue,
                            ord => {
                                res = ord;
                                break;
                            }
                        }
                    }
                    res
                }
                ord => ord,
            };
            match res {
                Ordering::Equal => 0,
                Ordering::Greater => 1,
                Ordering::Less => -1,
            }
        },
    );

    let find_at = |str: &[u8], substr: &[u8], pos: u64| {
        let pos = pos as usize;
        if pos == 0 {
            return 0_u64;
        }
        let p = pos - 1;
        if p + substr.len() <= str.len() {
            str[p..]
                .windows(substr.len())
                .position(|w| w == substr)
                .map_or(0, |i| i + 1 + p) as u64
        } else {
            0_u64
        }
    };
    registry.register_2_arg::<StringType, StringType, NumberType<u64>, _, _>(
        "instr",
        FunctionProperty::default(),
        |_, _| None,
        move |str: &[u8], substr: &[u8]| find_at(str, substr, 1),
    );

    registry.register_2_arg::<StringType, StringType, NumberType<u64>, _, _>(
        "position",
        FunctionProperty::default(),
        |_, _| None,
        move |substr: &[u8], str: &[u8]| find_at(str, substr, 1),
    );

    registry.register_2_arg::<StringType, StringType, NumberType<u64>, _, _>(
        "locate",
        FunctionProperty::default(),
        |_, _| None,
        move |substr: &[u8], str: &[u8]| find_at(str, substr, 1),
    );

    registry.register_3_arg::<StringType, StringType, NumberType<u64>, NumberType<u64>, _, _>(
        "locate",
        FunctionProperty::default(),
        |_, _, _| None,
        move |substr: &[u8], str: &[u8], pos: u64| find_at(str, substr, pos),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "to_base64",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len() * 4 / 3 + col.len() * 4,
            |val, writer| {
                base64::write::EncoderWriter::new(&mut writer.data, base64::STANDARD)
                    .write_all(val)
                    .unwrap();
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "from_base64",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len() * 4 / 3 + col.len() * 4,
            |val, writer| {
                base64::decode_config_buf(val, base64::STANDARD, &mut writer.data).unwrap();
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "quote",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len() * 2,
            |val, writer| {
                for ch in val {
                    match ch {
                        0 => writer.put_slice(&[b'\\', b'0']),
                        b'\'' => writer.put_slice(&[b'\\', b'\'']),
                        b'\"' => writer.put_slice(&[b'\\', b'\"']),
                        8 => writer.put_slice(&[b'\\', b'b']),
                        b'\n' => writer.put_slice(&[b'\\', b'n']),
                        b'\r' => writer.put_slice(&[b'\\', b'r']),
                        b'\t' => writer.put_slice(&[b'\\', b't']),
                        b'\\' => writer.put_slice(&[b'\\', b'\\']),
                        c => writer.put_u8(*c),
                    }
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "reverse",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len(),
            |val, writer| {
                let start = writer.data.len();
                writer.put_slice(val);
                let buf = &mut writer.data[start..];
                buf.reverse();
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_1_arg::<StringType, NumberType<u8>, _, _>(
        "ascii",
        FunctionProperty::default(),
        |domain| {
            Some(NumberDomain {
                min: domain.min.first().cloned().unwrap_or(0),
                max: domain
                    .max
                    .as_ref()
                    .map_or(u8::MAX, |v| v.first().cloned().unwrap_or_default()),
            })
        },
        |val| val.first().cloned().unwrap_or_default(),
    );

    // Trim functions
    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "ltrim",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len(),
            |val, writer| {
                let pos = val.iter().position(|ch| *ch != b' ' && *ch != b'\t');
                if let Some(idx) = pos {
                    writer.put_slice(&val.as_bytes()[idx..]);
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "rtrim",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len(),
            |val, writer| {
                let pos = val.iter().rev().position(|ch| *ch != b' ' && *ch != b'\t');
                if let Some(idx) = pos {
                    writer.put_slice(&val.as_bytes()[..val.len() - idx]);
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "trim",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len(),
            |val, writer| {
                let start_pos = val.iter().position(|ch| *ch != b' ' && *ch != b'\t');
                let end_pos = val.iter().rev().position(|ch| *ch != b' ' && *ch != b'\t');
                if let (Some(start_idx), Some(end_idx)) = (start_pos, end_pos) {
                    writer.put_slice(&val.as_bytes()[start_idx..val.len() - end_idx]);
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_2_arg::<StringType, StringType, StringType, _, _>(
        "trim_leading",
        FunctionProperty::default(),
        |_, _| None,
        vectorize_string_to_string_2_arg(
            |col, _| col.data.len(),
            |val, trim_str, writer| {
                let chunk_size = trim_str.len();
                let pos = val.chunks(chunk_size).position(|chunk| chunk != trim_str);
                if let Some(idx) = pos {
                    writer.put_slice(&val.as_bytes()[idx * chunk_size..]);
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_2_arg::<StringType, StringType, StringType, _, _>(
        "trim_trailing",
        FunctionProperty::default(),
        |_, _| None,
        vectorize_string_to_string_2_arg(
            |col, _| col.data.len(),
            |val, trim_str, writer| {
                let chunk_size = trim_str.len();
                let pos = val.rchunks(chunk_size).position(|chunk| chunk != trim_str);
                if let Some(idx) = pos {
                    writer.put_slice(&val.as_bytes()[..val.len() - idx * chunk_size]);
                }
                writer.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_2_arg::<StringType, StringType, StringType, _, _>(
        "trim_both",
        FunctionProperty::default(),
        |_, _| None,
        vectorize_string_to_string_2_arg(
            |col, _| col.data.len(),
            |val, trim_str, writer| {
                let chunk_size = trim_str.len();
                let start_pos = val.chunks(chunk_size).position(|chunk| chunk != trim_str);

                // Trim all
                if start_pos.is_none() {
                    writer.commit_row();
                    return Ok(());
                }

                let end_pos = val.rchunks(chunk_size).position(|chunk| chunk != trim_str);

                if let (Some(start_idx), Some(end_idx)) = (start_pos, end_pos) {
                    writer.put_slice(
                        &val.as_bytes()[start_idx * chunk_size..val.len() - end_idx * chunk_size],
                    );
                }

                writer.commit_row();
                Ok(())
            },
        ),
    );

    // TODO: generalize them to be alias of [CONV](https://dev.mysql.com/doc/refman/8.0/en/mathematical-functions.html#function_conv)
    // Tracking issue: https://github.com/datafuselabs/databend/issues/7242
    registry.register_passthrough_nullable_1_arg::<NumberType<i64>, StringType, _, _>(
        "bin",
        FunctionProperty::default(),
        |_| None,
        vectorize_with_builder_1_arg::<NumberType<i64>, StringType>(|val, output| {
            output.write_row(|data| write!(data, "{val:b}")).unwrap();
            Ok(())
        }),
    );
    registry.register_passthrough_nullable_1_arg::<NumberType<i64>, StringType, _, _>(
        "oct",
        FunctionProperty::default(),
        |_| None,
        vectorize_with_builder_1_arg::<NumberType<i64>, StringType>(|val, output| {
            output.write_row(|data| write!(data, "{val:o}")).unwrap();
            Ok(())
        }),
    );
    registry.register_passthrough_nullable_1_arg::<NumberType<i64>, StringType, _, _>(
        "hex",
        FunctionProperty::default(),
        |_| None,
        vectorize_with_builder_1_arg::<NumberType<i64>, StringType>(|val, output| {
            output.write_row(|data| write!(data, "{val:x}")).unwrap();
            Ok(())
        }),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "hex",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len() * 2,
            |val, output| {
                let old_len = output.data.len();
                let extra_len = val.len() * 2;
                output.data.resize(old_len + extra_len, 0);
                hex::encode_to_slice(val, &mut output.data[old_len..]).unwrap();
                output.commit_row();
                Ok(())
            },
        ),
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "unhex",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| col.data.len() / 2,
            |val, writer| {
                let old_len = writer.data.len();
                let extra_len = val.len() / 2;
                writer.data.resize(old_len + extra_len, 0);
                match hex::decode_to_slice(val, &mut writer.data[old_len..]) {
                    Ok(()) => {
                        writer.commit_row();
                        Ok(())
                    }
                    Err(err) => Err(format!(
                        "{:?} can not be `unhex()` because: {}",
                        String::from_utf8_lossy(val),
                        err
                    )),
                }
            },
        ),
    );

    registry.register_1_arg::<StringType, UInt64Type, _, _>(
        "ord",
        FunctionProperty::default(),
        |_| None,
        |str: &[u8]| {
            let mut res: u64 = 0;
            if !str.is_empty() {
                if str[0].is_ascii() {
                    res = str[0] as u64;
                } else {
                    for (p, _) in str.iter().enumerate() {
                        let s = &str[0..p + 1];
                        if std::str::from_utf8(s).is_ok() {
                            for (i, b) in s.iter().rev().enumerate() {
                                res += (*b as u64) * 256_u64.pow(i as u32);
                            }
                            break;
                        }
                    }
                }
            }
            res
        },
    );

    registry.register_passthrough_nullable_1_arg::<StringType, StringType, _, _>(
        "soundex",
        FunctionProperty::default(),
        |_| None,
        vectorize_string_to_string(
            |col| usize::max(col.data.len(), 4 * col.len()),
            |val, writer| {
                let mut last = None;
                let mut count = 0;

                for ch in String::from_utf8_lossy(val).chars() {
                    let score = Soundex::number_map(ch);
                    if last.is_none() {
                        if !Soundex::is_uni_alphabetic(ch) {
                            continue;
                        }
                        last = score;
                        writer.put_char(ch.to_ascii_uppercase());
                    } else {
                        if !ch.is_ascii_alphabetic()
                            || Soundex::is_drop(ch)
                            || score.is_none()
                            || score == last
                        {
                            continue;
                        }
                        last = score;
                        writer.put_char(score.unwrap() as char);
                    }

                    count += 1;
                }
                // add '0'
                for _ in count..4 {
                    writer.put_char('0');
                }

                writer.commit_row();
                Ok(())
            },
        ),
    );
}

// Vectorize string to string function with customer estimate_bytes.
fn vectorize_string_to_string(
    estimate_bytes: impl Fn(&StringColumn) -> usize + Copy,
    func: impl Fn(&[u8], &mut StringColumnBuilder) -> Result<(), String> + Copy,
) -> impl Fn(ValueRef<StringType>, &GenericMap) -> Result<Value<StringType>, String> + Copy {
    move |arg1, _| match arg1 {
        ValueRef::Scalar(val) => {
            let mut builder = StringColumnBuilder::with_capacity(1, 0);
            func(val, &mut builder)?;
            Ok(Value::Scalar(builder.build_scalar()))
        }
        ValueRef::Column(col) => {
            let data_capacity = estimate_bytes(&col);
            let mut builder = StringColumnBuilder::with_capacity(col.len(), data_capacity);
            for val in col.iter() {
                func(val, &mut builder)?;
            }

            Ok(Value::Column(builder.build()))
        }
    }
}

// Vectorize (string, string) -> string function with customer estimate_bytes.
fn vectorize_string_to_string_2_arg(
    estimate_bytes: impl Fn(&StringColumn, &StringColumn) -> usize + Copy,
    func: impl Fn(&[u8], &[u8], &mut StringColumnBuilder) -> Result<(), String> + Copy,
) -> impl Fn(
    ValueRef<StringType>,
    ValueRef<StringType>,
    &GenericMap,
) -> Result<Value<StringType>, String>
+ Copy {
    move |arg1, arg2, _| match (arg1, arg2) {
        (ValueRef::Scalar(arg1), ValueRef::Scalar(arg2)) => {
            let mut builder = StringColumnBuilder::with_capacity(1, 0);
            func(arg1, arg2, &mut builder)?;
            Ok(Value::Scalar(builder.build_scalar()))
        }
        (ValueRef::Scalar(arg1), ValueRef::Column(arg2)) => {
            let data_capacity =
                estimate_bytes(&StringColumnBuilder::repeat(arg1, 1).build(), &arg2);
            let mut builder = StringColumnBuilder::with_capacity(arg2.len(), data_capacity);
            for val in arg2.iter() {
                func(arg1, val, &mut builder)?;
            }
            Ok(Value::Column(builder.build()))
        }
        (ValueRef::Column(arg1), ValueRef::Scalar(arg2)) => {
            let data_capacity =
                estimate_bytes(&arg1, &StringColumnBuilder::repeat(arg2, 1).build());
            let mut builder = StringColumnBuilder::with_capacity(arg1.len(), data_capacity);
            for val in arg1.iter() {
                func(val, arg2, &mut builder)?;
            }
            Ok(Value::Column(builder.build()))
        }
        (ValueRef::Column(arg1), ValueRef::Column(arg2)) => {
            let data_capacity = estimate_bytes(&arg1, &arg2);
            let mut builder = StringColumnBuilder::with_capacity(arg1.len(), data_capacity);
            let iter = arg1.iter().zip(arg2.iter());
            for (val1, val2) in iter {
                func(val1, val2, &mut builder)?;
            }
            Ok(Value::Column(builder.build()))
        }
    }
}
