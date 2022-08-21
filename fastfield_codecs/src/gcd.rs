use std::io::{self, Write};
use std::num::NonZeroU64;

use common::BinarySerializable;
use fastdivide::DividerU64;

use crate::FastFieldCodecReader;

/// Wrapper for accessing a fastfield.
///
/// Holds the data and the codec to the read the data.
#[derive(Clone)]
pub struct GCDFastFieldCodecReader<CodecReader> {
    pub params: GCDParams,
    pub reader: CodecReader,
}

impl<C: FastFieldCodecReader> FastFieldCodecReader for GCDFastFieldCodecReader<C> {
    #[inline]
    fn get_u64(&self, doc: u64) -> u64 {
        self.params.min_value + self.params.gcd.get() * self.reader.get_u64(doc)
    }

    fn min_value(&self) -> u64 {
        self.params.min_value + self.params.gcd.get() * self.reader.min_value()
    }

    fn max_value(&self) -> u64 {
        self.params.min_value + self.params.gcd.get() * self.reader.max_value()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct GCDParams {
    pub min_value: u64,
    pub gcd: NonZeroU64,
}

impl BinarySerializable for GCDParams {
    fn serialize<W: Write>(&self, wrt: &mut W) -> io::Result<()> {
        self.gcd.get().serialize(wrt)?;
        self.min_value.serialize(wrt)?;
        Ok(())
    }

    fn deserialize<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let gcd = NonZeroU64::new(u64::deserialize(reader)?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "GCD=0 is invalid."))?;
        let min_value = u64::deserialize(reader)?;
        Ok(GCDParams { min_value, gcd })
    }
}

fn compute_gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

// Find GCD for iterator of numbers
//
// If all numbers are '0' (or if there are not numbers, return None).
pub fn find_gcd(numbers: impl Iterator<Item = u64>) -> Option<NonZeroU64> {
    let mut numbers = numbers.filter(|n| *n != 0);
    let mut gcd = numbers.next()?;
    if gcd == 1 {
        return NonZeroU64::new(gcd);
    }

    let mut gcd_divider = DividerU64::divide_by(gcd);
    for val in numbers {
        let remainder = val - gcd_divider.divide(val) * gcd;
        if remainder == 0 {
            continue;
        }
        gcd = compute_gcd(gcd, val);
        if gcd == 1 {
            return NonZeroU64::new(1);
        }
        gcd_divider = DividerU64::divide_by(gcd);
    }
    NonZeroU64::new(gcd)
}

#[cfg(test)]
mod tests {

    // TODO Move test
    //
    // use std::collections::HashMap;
    // use std::path::Path;
    //
    // use crate::directory::{CompositeFile, RamDirectory, WritePtr};
    // use crate::fastfield::serializer::FastFieldCodecEnableCheck;
    // use crate::fastfield::tests::{FIELD, FIELDI64, SCHEMA, SCHEMAI64};
    // use super::{
    // find_gcd, CompositeFastFieldSerializer, DynamicFastFieldReader, FastFieldCodecName,
    // FastFieldReader, FastFieldsWriter, ALL_CODECS,
    // };
    // use crate::schema::Schema;
    // use crate::Directory;
    //
    // fn get_index(
    // docs: &[crate::Document],
    // schema: &Schema,
    // codec_enable_checker: FastFieldCodecEnableCheck,
    // ) -> crate::Result<RamDirectory> {
    // let directory: RamDirectory = RamDirectory::create();
    // {
    // let write: WritePtr = directory.open_write(Path::new("test")).unwrap();
    // let mut serializer =
    // CompositeFastFieldSerializer::from_write_with_codec(write, codec_enable_checker)
    // .unwrap();
    // let mut fast_field_writers = FastFieldsWriter::from_schema(schema);
    // for doc in docs {
    // fast_field_writers.add_document(doc);
    // }
    // fast_field_writers
    // .serialize(&mut serializer, &HashMap::new(), None)
    // .unwrap();
    // serializer.close().unwrap();
    // }
    // Ok(directory)
    // }
    //
    // fn test_fastfield_gcd_i64_with_codec(
    // codec_name: FastFieldCodecName,
    // num_vals: usize,
    // ) -> crate::Result<()> {
    // let path = Path::new("test");
    // let mut docs = vec![];
    // for i in 1..=num_vals {
    // let val = i as i64 * 1000i64;
    // docs.push(doc!(*FIELDI64=>val));
    // }
    // let directory = get_index(&docs, &SCHEMAI64, codec_name.clone().into())?;
    // let file = directory.open_read(path).unwrap();
    // assert_eq!(file.len(), 118);
    // let composite_file = CompositeFile::open(&file)?;
    // let file = composite_file.open_read(*FIELD).unwrap();
    // let fast_field_reader = DynamicFastFieldReader::<i64>::open(file)?;
    // assert_eq!(fast_field_reader.get(0), 1000i64);
    // assert_eq!(fast_field_reader.get(1), 2000i64);
    // assert_eq!(fast_field_reader.get(2), 3000i64);
    // assert_eq!(fast_field_reader.max_value(), num_vals as i64 * 1000);
    // assert_eq!(fast_field_reader.min_value(), 1000i64);
    // let file = directory.open_read(path).unwrap();
    //
    // Can't apply gcd
    // let path = Path::new("test");
    // docs.pop();
    // docs.push(doc!(*FIELDI64=>2001i64));
    // let directory = get_index(&docs, &SCHEMAI64, codec_name.into())?;
    // let file2 = directory.open_read(path).unwrap();
    // assert!(file2.len() > file.len());
    //
    // Ok(())
    // }
    //
    // #[test]
    // fn test_fastfield_gcd_i64() -> crate::Result<()> {
    // for codec_name in ALL_CODECS {
    // test_fastfield_gcd_i64_with_codec(codec_name.clone(), 5005)?;
    // }
    // Ok(())
    // }
    //
    // fn test_fastfield_gcd_u64_with_codec(
    // codec_name: FastFieldCodecName,
    // num_vals: usize,
    // ) -> crate::Result<()> {
    // let path = Path::new("test");
    // let mut docs = vec![];
    // for i in 1..=num_vals {
    // let val = i as u64 * 1000u64;
    // docs.push(doc!(*FIELD=>val));
    // }
    // let directory = get_index(&docs, &SCHEMA, codec_name.clone().into())?;
    // let file = directory.open_read(path).unwrap();
    // assert_eq!(file.len(), 118);
    // let composite_file = CompositeFile::open(&file)?;
    // let file = composite_file.open_read(*FIELD).unwrap();
    // let fast_field_reader = DynamicFastFieldReader::<u64>::open(file)?;
    // assert_eq!(fast_field_reader.get(0), 1000u64);
    // assert_eq!(fast_field_reader.get(1), 2000u64);
    // assert_eq!(fast_field_reader.get(2), 3000u64);
    // assert_eq!(fast_field_reader.max_value(), num_vals as u64 * 1000);
    // assert_eq!(fast_field_reader.min_value(), 1000u64);
    // let file = directory.open_read(path).unwrap();
    //
    // Can't apply gcd
    // let path = Path::new("test");
    // docs.pop();
    // docs.push(doc!(*FIELDI64=>2001u64));
    // let directory = get_index(&docs, &SCHEMA, codec_name.into())?;
    // let file2 = directory.open_read(path).unwrap();
    // assert!(file2.len() > file.len());
    //
    // Ok(())
    // }
    //
    // #[test]
    // fn test_fastfield_gcd_u64() -> crate::Result<()> {
    // for codec_name in ALL_CODECS {
    // test_fastfield_gcd_u64_with_codec(codec_name.clone(), 5005)?;
    // }
    // Ok(())
    // }
    //
    // #[test]
    // pub fn test_fastfield2() {
    // let test_fastfield = DynamicFastFieldReader::<u64>::from(vec![100, 200, 300]);
    // assert_eq!(test_fastfield.get(0), 100);
    // assert_eq!(test_fastfield.get(1), 200);
    // assert_eq!(test_fastfield.get(2), 300);
    // }

    use std::num::NonZeroU64;

    use crate::gcd::{compute_gcd, find_gcd};

    #[test]
    fn test_compute_gcd() {
        assert_eq!(compute_gcd(0, 0), 0);
        assert_eq!(compute_gcd(4, 0), 4);
        assert_eq!(compute_gcd(0, 4), 4);
        assert_eq!(compute_gcd(1, 4), 1);
        assert_eq!(compute_gcd(4, 1), 1);
        assert_eq!(compute_gcd(4, 2), 2);
        assert_eq!(compute_gcd(10, 25), 5);
        assert_eq!(compute_gcd(25, 10), 5);
        assert_eq!(compute_gcd(25, 25), 25);
    }

    #[test]
    fn find_gcd_test() {
        assert_eq!(find_gcd([0].into_iter()), None);
        assert_eq!(find_gcd([0, 10].into_iter()), NonZeroU64::new(10));
        assert_eq!(find_gcd([10, 0].into_iter()), NonZeroU64::new(10));
        assert_eq!(find_gcd([].into_iter()), None);
        assert_eq!(find_gcd([15, 30, 5, 10].into_iter()), NonZeroU64::new(5));
        assert_eq!(find_gcd([15, 16, 10].into_iter()), NonZeroU64::new(1));
        assert_eq!(find_gcd([0, 5, 5, 5].into_iter()), NonZeroU64::new(5));
        assert_eq!(find_gcd([0, 0].into_iter()), None);
    }
}