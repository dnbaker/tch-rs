//! Numpy support for tensors.
//!
//! Format spec:
//! https://docs.scipy.org/doc/numpy-1.14.2/neps/npy-format.html
use crate::{Kind, Tensor};
use failure::Fallible;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};

static NPY_MAGIC_STRING: &[u8] = b"\x93NUMPY";
static NPY_SUFFIX: &str = ".npy";

fn read_header<R: Read>(buf_reader: &mut BufReader<R>) -> Fallible<String> {
    let mut magic_string = vec![0u8; NPY_MAGIC_STRING.len()];
    buf_reader.read_exact(&mut magic_string)?;
    ensure!(magic_string == NPY_MAGIC_STRING, "magic string mismatch");
    let mut version = [0u8; 2];
    buf_reader.read_exact(&mut version)?;
    let header_len_len = match version[0] {
        1 => 2,
        2 => 4,
        otherwise => bail!("unsupported version {}", otherwise),
    };
    let mut header_len = vec![0u8; header_len_len];
    buf_reader.read_exact(&mut header_len)?;
    let header_len = header_len
        .iter()
        .rev()
        .fold(0 as usize, |acc, &v| 256 * acc + v as usize);
    let mut header = vec![0u8; header_len];
    buf_reader.read_exact(&mut header)?;
    Ok(String::from_utf8_lossy(&header).to_string())
}

#[derive(Debug, PartialEq)]
struct Header {
    descr: Kind,
    fortran_order: bool,
    shape: Vec<i64>,
}

impl Header {
    // Hacky parser for the npy header, a typical example would be:
    // {'descr': '<f8', 'fortran_order': False, 'shape': (128,), }
    fn parse(header: &str) -> Fallible<Header> {
        let header =
            header.trim_matches(|c: char| c == '{' || c == '}' || c == ',' || c.is_whitespace());

        let mut parts: Vec<String> = vec![];
        let mut start_index = 0usize;
        let mut cnt_parenthesis = 0i64;
        for (index, c) in header.chars().enumerate() {
            match c {
                '(' => cnt_parenthesis += 1,
                ')' => cnt_parenthesis -= 1,
                ',' => {
                    if cnt_parenthesis == 0 {
                        parts.push(header[start_index..index].to_owned());
                        start_index = index + 1;
                    }
                }
                _ => {}
            }
        }
        parts.push(header[start_index..].to_owned());
        let mut part_map: HashMap<String, String> = HashMap::new();
        for part in parts.iter() {
            let part = part.trim();
            if part.len() > 0 {
                match part.split(':').collect::<Vec<_>>().as_slice() {
                    [key, value] => {
                        let key = key.trim_matches(|c: char| c == '\'' || c.is_whitespace());
                        let value = value.trim_matches(|c: char| c == '\'' || c.is_whitespace());
                        let _ = part_map.insert(key.to_owned(), value.to_owned());
                    }
                    _ => bail!("unable to parse header {}", header),
                }
            }
        }
        let fortran_order = match part_map.get("fortran_order") {
            None => false,
            Some(fortran_order) => match fortran_order.as_ref() {
                "False" => false,
                "True" => true,
                _ => bail!("unknown fortran_order {}", fortran_order),
            },
        };
        let descr = match part_map.get("descr") {
            None => bail!("no descr in header"),
            Some(descr) => {
                ensure!(!descr.is_empty(), "empty descr");
                ensure!(
                    descr.chars().next().unwrap() != '>',
                    "little-endian descr {}",
                    descr
                );
                match descr.trim_matches(|c: char| c == '=' || c == '<') {
                    "f4" => Kind::Float,
                    "f8" => Kind::Double,
                    "i4" => Kind::Int,
                    "i8" => Kind::Int64,
                    "i2" => Kind::Int16,
                    "i1" => Kind::Int8,
                    "u1" => Kind::Uint8,
                    descr => bail!("unrecognized descr {}", descr),
                }
            }
        };
        let shape = match part_map.get("shape") {
            None => bail!("no shape in header"),
            Some(shape) => {
                let shape = shape.trim_matches(|c: char| c == '(' || c == ')' || c == ',');
                shape
                    .split(',')
                    .map(|v| v.trim().parse::<i64>())
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        Ok(Header {
            descr,
            fortran_order,
            shape,
        })
    }
}

impl crate::Tensor {
    pub fn read_npy<T: AsRef<std::path::Path>>(path: T) -> Fallible<Tensor> {
        let mut buf_reader = BufReader::new(File::open(path.as_ref())?);
        let header = read_header(&mut buf_reader)?;
        let header = Header::parse(&header)?;
        ensure!(!header.fortran_order, "fortran order not supported");
        let mut data: Vec<u8> = vec![];
        buf_reader.read_to_end(&mut data)?;
        Tensor::f_of_data_size(&data, &header.shape, header.descr)
    }

    pub fn read_npz<T: AsRef<std::path::Path>>(path: T) -> Fallible<Vec<(String, Tensor)>> {
        let zip_reader = BufReader::new(File::open(path.as_ref())?);
        let mut zip = zip::ZipArchive::new(zip_reader)?;
        let mut result = vec![];
        for i in 0..zip.len() {
            let file = zip.by_index(i).unwrap();
            let name = {
                let name = file.name();
                if name.ends_with(NPY_SUFFIX) {
                    name[..name.len() - NPY_SUFFIX.len()].to_owned()
                } else {
                    name.to_owned()
                }
            };
            let mut buf_reader = BufReader::new(file);
            let header = read_header(&mut buf_reader)?;
            let header = Header::parse(&header)?;
            ensure!(!header.fortran_order, "fortran order not supported");
            let mut data: Vec<u8> = vec![];
            buf_reader.read_to_end(&mut data)?;
            let tensor = Tensor::f_of_data_size(&data, &header.shape, header.descr)?;
            result.push((name, tensor))
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::Header;

    #[test]
    fn parse() {
        let h = "{'descr': '<f8', 'fortran_order': False, 'shape': (128,), }";
        assert_eq!(
            Header::parse(h).unwrap(),
            Header {
                descr: crate::Kind::Double,
                fortran_order: false,
                shape: vec![128]
            }
        );
        let h = "{'descr': '<f4', 'fortran_order': True, 'shape': (256,1,128), }";
        assert_eq!(
            Header::parse(h).unwrap(),
            Header {
                descr: crate::Kind::Float,
                fortran_order: true,
                shape: vec![256, 1, 128]
            }
        )
    }
}
