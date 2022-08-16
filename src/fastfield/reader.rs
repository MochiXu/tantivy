use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::RangeInclusive;
use std::path::Path;

use fastfield_codecs::bitpacked::{
    BitpackedFastFieldReader as BitpackedReader, BitpackedFastFieldSerializer,
};
use fastfield_codecs::linearinterpol::{
    LinearInterpolFastFieldReader, LinearInterpolFastFieldSerializer,
};
use fastfield_codecs::multilinearinterpol::{
    MultiLinearInterpolFastFieldReader, MultiLinearInterpolFastFieldSerializer,
};
use fastfield_codecs::{FastFieldCodecReader, FastFieldCodecReaderU128, FastFieldCodecSerializer};

use super::{FastValue, FastValueU128, GCDFastFieldCodec, GCD_CODEC_ID};
use crate::directory::{CompositeFile, Directory, FileSlice, OwnedBytes, RamDirectory, WritePtr};
use crate::fastfield::{CompositeFastFieldSerializer, FastFieldsWriter};
use crate::schema::{Schema, FAST};
use crate::DocId;

/// FastFieldReader is the trait to access fast field data.
pub trait FastFieldReader<Item: FastValue>: Clone {
    /// Return the value associated to the given document.
    ///
    /// This accessor should return as fast as possible.
    ///
    /// # Panics
    ///
    /// May panic if `doc` is greater than the segment
    fn get(&self, doc: DocId) -> Item;

    /// Fills an output buffer with the fast field values
    /// associated with the `DocId` going from
    /// `start` to `start + output.len()`.
    ///
    /// Regardless of the type of `Item`, this method works
    /// - transmuting the output array
    /// - extracting the `Item`s as if they were `u64`
    /// - possibly converting the `u64` value to the right type.
    ///
    /// # Panics
    ///
    /// May panic if `start + output.len()` is greater than
    /// the segment's `maxdoc`.
    fn get_range(&self, start: u64, output: &mut [Item]);

    /// Returns the minimum value for this fast field.
    ///
    /// The min value does not take in account of possible
    /// deleted document, and should be considered as a lower bound
    /// of the actual mimimum value.
    fn min_value(&self) -> Item;

    /// Returns the maximum value for this fast field.
    ///
    /// The max value does not take in account of possible
    /// deleted document, and should be considered as an upper bound
    /// of the actual maximum value.
    fn max_value(&self) -> Item;
}

#[derive(Clone)]
/// DynamicFastFieldReader wraps different readers to access
/// the various encoded fastfield data
pub enum DynamicFastFieldReader<Item: FastValue> {
    /// Bitpacked compressed fastfield data.
    Bitpacked(FastFieldReaderCodecWrapper<Item, BitpackedReader>),
    /// Linear interpolated values + bitpacked
    LinearInterpol(FastFieldReaderCodecWrapper<Item, LinearInterpolFastFieldReader>),
    /// Blockwise linear interpolated values + bitpacked
    MultiLinearInterpol(FastFieldReaderCodecWrapper<Item, MultiLinearInterpolFastFieldReader>),

    /// GCD and Bitpacked compressed fastfield data.
    BitpackedGCD(FastFieldReaderCodecWrapper<Item, GCDFastFieldCodec<BitpackedReader>>),
    /// GCD and Linear interpolated values + bitpacked
    LinearInterpolGCD(
        FastFieldReaderCodecWrapper<Item, GCDFastFieldCodec<LinearInterpolFastFieldReader>>,
    ),
    /// GCD and Blockwise linear interpolated values + bitpacked
    MultiLinearInterpolGCD(
        FastFieldReaderCodecWrapper<Item, GCDFastFieldCodec<MultiLinearInterpolFastFieldReader>>,
    ),
}

impl<Item: FastValue> DynamicFastFieldReader<Item> {
    /// Returns correct the reader wrapped in the `DynamicFastFieldReader` enum for the data.
    pub fn open_from_id(
        mut bytes: OwnedBytes,
        codec_id: u8,
    ) -> crate::Result<DynamicFastFieldReader<Item>> {
        let reader = match codec_id {
            BitpackedFastFieldSerializer::ID => {
                DynamicFastFieldReader::Bitpacked(FastFieldReaderCodecWrapper::<
                    Item,
                    BitpackedReader,
                >::open_from_bytes(bytes)?)
            }
            LinearInterpolFastFieldSerializer::ID => {
                DynamicFastFieldReader::LinearInterpol(FastFieldReaderCodecWrapper::<
                    Item,
                    LinearInterpolFastFieldReader,
                >::open_from_bytes(bytes)?)
            }
            MultiLinearInterpolFastFieldSerializer::ID => {
                DynamicFastFieldReader::MultiLinearInterpol(FastFieldReaderCodecWrapper::<
                    Item,
                    MultiLinearInterpolFastFieldReader,
                >::open_from_bytes(
                    bytes
                )?)
            }
            _ if codec_id == GCD_CODEC_ID => {
                let codec_id = bytes.read_u8();

                match codec_id {
                    BitpackedFastFieldSerializer::ID => {
                        DynamicFastFieldReader::BitpackedGCD(FastFieldReaderCodecWrapper::<
                            Item,
                            GCDFastFieldCodec<BitpackedReader>,
                        >::open_from_bytes(
                            bytes
                        )?)
                    }
                    LinearInterpolFastFieldSerializer::ID => {
                        DynamicFastFieldReader::LinearInterpolGCD(FastFieldReaderCodecWrapper::<
                            Item,
                            GCDFastFieldCodec<LinearInterpolFastFieldReader>,
                        >::open_from_bytes(
                            bytes
                        )?)
                    }
                    MultiLinearInterpolFastFieldSerializer::ID => {
                        DynamicFastFieldReader::MultiLinearInterpolGCD(
                            FastFieldReaderCodecWrapper::<
                                Item,
                                GCDFastFieldCodec<MultiLinearInterpolFastFieldReader>,
                            >::open_from_bytes(bytes)?,
                        )
                    }
                    _ => {
                        panic!(
                            "unknown fastfield codec id {:?}. Data corrupted or using old tantivy \
                             version.",
                            codec_id
                        )
                    }
                }
            }
            _ => {
                panic!(
                    "unknown fastfield codec id {:?}. Data corrupted or using old tantivy version.",
                    codec_id
                )
            }
        };
        Ok(reader)
    }
    /// Returns correct the reader wrapped in the `DynamicFastFieldReader` enum for the data.
    pub fn open(file: FileSlice) -> crate::Result<DynamicFastFieldReader<Item>> {
        let mut bytes = file.read_bytes()?;
        let codec_id = bytes.read_u8();

        Self::open_from_id(bytes, codec_id)
    }
}

impl<Item: FastValue> FastFieldReader<Item> for DynamicFastFieldReader<Item> {
    #[inline]
    fn get(&self, doc: DocId) -> Item {
        match self {
            Self::Bitpacked(reader) => reader.get(doc),
            Self::LinearInterpol(reader) => reader.get(doc),
            Self::MultiLinearInterpol(reader) => reader.get(doc),
            Self::BitpackedGCD(reader) => reader.get(doc),
            Self::LinearInterpolGCD(reader) => reader.get(doc),
            Self::MultiLinearInterpolGCD(reader) => reader.get(doc),
        }
    }
    #[inline]
    fn get_range(&self, start: u64, output: &mut [Item]) {
        match self {
            Self::Bitpacked(reader) => reader.get_range(start, output),
            Self::LinearInterpol(reader) => reader.get_range(start, output),
            Self::MultiLinearInterpol(reader) => reader.get_range(start, output),
            Self::BitpackedGCD(reader) => reader.get_range(start, output),
            Self::LinearInterpolGCD(reader) => reader.get_range(start, output),
            Self::MultiLinearInterpolGCD(reader) => reader.get_range(start, output),
        }
    }
    fn min_value(&self) -> Item {
        match self {
            Self::Bitpacked(reader) => reader.min_value(),
            Self::LinearInterpol(reader) => reader.min_value(),
            Self::MultiLinearInterpol(reader) => reader.min_value(),
            Self::BitpackedGCD(reader) => reader.min_value(),
            Self::LinearInterpolGCD(reader) => reader.min_value(),
            Self::MultiLinearInterpolGCD(reader) => reader.min_value(),
        }
    }
    fn max_value(&self) -> Item {
        match self {
            Self::Bitpacked(reader) => reader.max_value(),
            Self::LinearInterpol(reader) => reader.max_value(),
            Self::MultiLinearInterpol(reader) => reader.max_value(),
            Self::BitpackedGCD(reader) => reader.max_value(),
            Self::LinearInterpolGCD(reader) => reader.max_value(),
            Self::MultiLinearInterpolGCD(reader) => reader.max_value(),
        }
    }
}

/// Wrapper for accessing a fastfield.
///
/// Holds the data and the codec to the read the data.
#[derive(Clone)]
pub struct FastFieldReaderCodecWrapperU128<Item: FastValueU128, CodecReader> {
    reader: CodecReader,
    bytes: OwnedBytes,
    _phantom: PhantomData<Item>,
}

impl<Item: FastValueU128, C: FastFieldCodecReaderU128> FastFieldReaderCodecWrapperU128<Item, C> {
    /// Opens a fast field given the bytes.
    pub fn open_from_bytes(bytes: OwnedBytes) -> crate::Result<Self> {
        let reader = C::open_from_bytes(bytes.as_slice())?;
        Ok(Self {
            reader,
            bytes,
            _phantom: PhantomData,
        })
    }

    /// Returns the item for the docid
    pub fn get(&self, doc: DocId) -> Option<Item> {
        self.reader
            .get(doc as u64, self.bytes.as_slice())
            .map(|el| Item::from_u128(el))
    }

    /// Iterates over all elements in the fast field
    pub fn iter(&self) -> impl Iterator<Item = Option<Item>> + '_ {
        self.reader
            .iter(self.bytes.as_slice())
            .map(|el| el.map(Item::from_u128))
    }

    /// Returns all docids which are in the provided range
    pub fn get_range(&self, range: RangeInclusive<u128>) -> Vec<usize> {
        self.reader.get_range(range, self.bytes.as_slice())
    }
}

/// Wrapper for accessing a fastfield.
///
/// Holds the data and the codec to the read the data.
#[derive(Clone)]
pub struct FastFieldReaderCodecWrapper<Item: FastValue, CodecReader> {
    reader: CodecReader,
    bytes: OwnedBytes,
    _phantom: PhantomData<Item>,
}

impl<Item: FastValue, C: FastFieldCodecReader> FastFieldReaderCodecWrapper<Item, C> {
    /// Opens a fast field given a file.
    pub fn open(file: FileSlice) -> crate::Result<Self> {
        let mut bytes = file.read_bytes()?;
        let codec_id = bytes.read_u8();
        assert_eq!(
            BitpackedFastFieldSerializer::ID,
            codec_id,
            "Tried to open fast field as bitpacked encoded (id=1), but got serializer with \
             different id"
        );
        Self::open_from_bytes(bytes)
    }
    /// Opens a fast field given the bytes.
    pub fn open_from_bytes(bytes: OwnedBytes) -> crate::Result<Self> {
        let reader = C::open_from_bytes(bytes.as_slice())?;
        Ok(FastFieldReaderCodecWrapper {
            reader,
            bytes,
            _phantom: PhantomData,
        })
    }
    #[inline]
    pub(crate) fn get_u64(&self, doc: u64) -> Item {
        let data = self.reader.get_u64(doc, self.bytes.as_slice());
        Item::from_u64(data)
    }

    /// Internally `multivalued` also use SingleValue Fast fields.
    /// It works as follows... A first column contains the list of start index
    /// for each document, a second column contains the actual values.
    ///
    /// The values associated to a given doc, are then
    ///  `second_column[first_column.get(doc)..first_column.get(doc+1)]`.
    ///
    /// Which means single value fast field reader can be indexed internally with
    /// something different from a `DocId`. For this use case, we want to use `u64`
    /// values.
    ///
    /// See `get_range` for an actual documentation about this method.
    pub(crate) fn get_range_u64(&self, start: u64, output: &mut [Item]) {
        for (i, out) in output.iter_mut().enumerate() {
            *out = self.get_u64(start + (i as u64));
        }
    }
}

impl<Item: FastValue, C: FastFieldCodecReader + Clone> FastFieldReader<Item>
    for FastFieldReaderCodecWrapper<Item, C>
{
    /// Return the value associated to the given document.
    ///
    /// This accessor should return as fast as possible.
    ///
    /// # Panics
    ///
    /// May panic if `doc` is greater than the segment
    // `maxdoc`.
    fn get(&self, doc: DocId) -> Item {
        self.get_u64(u64::from(doc))
    }

    /// Fills an output buffer with the fast field values
    /// associated with the `DocId` going from
    /// `start` to `start + output.len()`.
    ///
    /// Regardless of the type of `Item`, this method works
    /// - transmuting the output array
    /// - extracting the `Item`s as if they were `u64`
    /// - possibly converting the `u64` value to the right type.
    ///
    /// # Panics
    ///
    /// May panic if `start + output.len()` is greater than
    /// the segment's `maxdoc`.
    fn get_range(&self, start: u64, output: &mut [Item]) {
        self.get_range_u64(start, output);
    }

    /// Returns the minimum value for this fast field.
    ///
    /// The max value does not take in account of possible
    /// deleted document, and should be considered as an upper bound
    /// of the actual maximum value.
    fn min_value(&self) -> Item {
        Item::from_u64(self.reader.min_value())
    }

    /// Returns the maximum value for this fast field.
    ///
    /// The max value does not take in account of possible
    /// deleted document, and should be considered as an upper bound
    /// of the actual maximum value.
    fn max_value(&self) -> Item {
        Item::from_u64(self.reader.max_value())
    }
}

impl<Item: FastValue> From<Vec<Item>> for DynamicFastFieldReader<Item> {
    fn from(vals: Vec<Item>) -> DynamicFastFieldReader<Item> {
        let mut schema_builder = Schema::builder();
        let field = schema_builder.add_u64_field("field", FAST);
        let schema = schema_builder.build();
        let path = Path::new("__dummy__");
        let directory: RamDirectory = RamDirectory::create();
        {
            let write: WritePtr = directory
                .open_write(path)
                .expect("With a RamDirectory, this should never fail.");
            let mut serializer = CompositeFastFieldSerializer::from_write(write)
                .expect("With a RamDirectory, this should never fail.");
            let mut fast_field_writers = FastFieldsWriter::from_schema(&schema);
            {
                let fast_field_writer = fast_field_writers
                    .get_field_writer_mut(field)
                    .expect("With a RamDirectory, this should never fail.");
                for val in vals {
                    fast_field_writer.add_val(val.to_u64());
                }
            }
            fast_field_writers
                .serialize(&mut serializer, &HashMap::new(), None)
                .unwrap();
            serializer.close().unwrap();
        }

        let file = directory.open_read(path).expect("Failed to open the file");
        let composite_file = CompositeFile::open(&file).expect("Failed to read the composite file");
        let field_file = composite_file
            .open_read(field)
            .expect("File component not found");
        DynamicFastFieldReader::open(field_file).unwrap()
    }
}
