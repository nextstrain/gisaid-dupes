use crate::io::fs::extension;
use crate::utils::error::report_to_string;
use bzip2::read::MultiBzDecoder;
use bzip2::write::BzEncoder;
use bzip2::Compression as BzCompressionLevel;
use color_eyre::{Help, SectionExt};
use eyre::{Report, WrapErr};
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression as GzCompressionLevel;
use log::debug;
use std::env;
use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::str::FromStr;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use zstd::Decoder as ZstdDecoder;
use zstd::Encoder as ZstdEncoder;

#[derive(strum_macros::Display, Clone)]
pub enum CompressionType {
  Bzip2,
  Xz,
  Zstandard,
  Gzip,
  None,
}

pub fn guess_compression_from_filepath(filepath: impl AsRef<Path>) -> (CompressionType, String) {
  let filepath = filepath.as_ref();

  match extension(filepath).map(|ext| ext.to_lowercase()) {
    None => (CompressionType::None, "".to_owned()),
    Some(ext) => {
      let compression_type: CompressionType = match ext.as_str() {
        "bz2" => CompressionType::Bzip2,
        "xz" => CompressionType::Xz,
        "zst" => CompressionType::Zstandard,
        "gz" => CompressionType::Gzip,
        _ => CompressionType::None,
      };

      debug!(
        "When processing '{filepath:#?}': detected file extension '{ext}'. \
        It will be using algorithm: '{compression_type}'"
      );

      (compression_type, ext)
    }
  }
}

pub struct Decompressor<'r> {
  decompressor: Box<dyn Read + 'r>,
  compression_type: CompressionType,
  filepath: Option<String>,
}

impl<'r> Decompressor<'r> {
  pub fn new<R: 'r + Read>(reader: R, compression_type: &CompressionType) -> Result<Self, Report> {
    let decompressor: Box<dyn Read> = match compression_type {
      CompressionType::Bzip2 => Box::new(MultiBzDecoder::new(reader)),
      CompressionType::Xz => Box::new(XzDecoder::new_multi_decoder(reader)),
      CompressionType::Zstandard => Box::new(ZstdDecoder::new(reader)?),
      CompressionType::Gzip => Box::new(MultiGzDecoder::new(reader)),
      CompressionType::None => Box::new(reader),
    };

    Ok(Self {
      decompressor,
      compression_type: compression_type.clone(),
      filepath: None,
    })
  }

  pub fn from_str_and_path(content: &'r str, filepath: impl AsRef<Path>) -> Result<Self, Report> {
    let filepath = filepath.as_ref();
    let reader = content.as_bytes();
    let (compression_type, _) = guess_compression_from_filepath(filepath);
    Self::new(reader, &compression_type)
  }

  pub fn from_path<R: 'r + Read>(reader: R, filepath: impl AsRef<Path>) -> Result<Self, Report> {
    let filepath = filepath.as_ref();
    let (compression_type, _) = guess_compression_from_filepath(filepath);
    Self::new(reader, &compression_type)
  }
}

impl<'r> Read for Decompressor<'r> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    self
      .decompressor
      .read(buf)
      .wrap_err_with(|| "While decompressing file")
      .with_section(|| {
        self
          .filepath
          .clone()
          .unwrap_or_else(|| "None".to_owned())
          .header("Filename")
      })
      .with_section(|| self.compression_type.clone().header("Decompressor"))
      .map_err(|report| std::io::Error::new(ErrorKind::Other, report_to_string(&report)))
  }
}

fn get_comp_level<I: FromStr + From<u8>>(ext: &str) -> I {
  let var_name = format!("{}_COMPRESSION", ext.to_uppercase());
  env::var(var_name)
    .ok()
    .and_then(|val| val.parse::<I>().ok())
    .unwrap_or_else(|| I::from(2))
}

pub struct Compressor<'w> {
  compressor: Box<dyn Write + Send + 'w>,
  compression_type: CompressionType,
  filepath: Option<String>,
}

impl<'w> Compressor<'w> {
  pub fn new<W: 'w + Write + Send>(writer: W, compression_type: &CompressionType) -> Result<Self, Report> {
    let compressor: Box<dyn Write + Send + 'w> = match compression_type {
      CompressionType::Bzip2 => Box::new(BzEncoder::new(writer, BzCompressionLevel::new(get_comp_level("BZ2")))),
      CompressionType::Xz => Box::new(XzEncoder::new(writer, get_comp_level("XZ"))),
      CompressionType::Zstandard => Box::new(ZstdEncoder::new(writer, get_comp_level("ZST"))?.auto_finish()),
      CompressionType::Gzip => Box::new(GzEncoder::new(writer, GzCompressionLevel::new(get_comp_level("GZ")))),
      CompressionType::None => Box::new(writer),
    };

    Ok(Self {
      compressor,
      compression_type: compression_type.clone(),
      filepath: None,
    })
  }

  pub fn from_path<W: 'w + Write + Send>(writer: W, filepath: impl AsRef<Path>) -> Result<Self, Report> {
    let filepath = filepath.as_ref();
    let (compression_type, _) = guess_compression_from_filepath(filepath);
    Self::new(writer, &compression_type)
  }
}

impl<'w> Write for Compressor<'w> {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    self
      .compressor
      .write(buf)
      .wrap_err_with(|| "While compressing file")
      .with_section(|| {
        self
          .filepath
          .clone()
          .unwrap_or_else(|| "None".to_owned())
          .header("Filename")
      })
      .with_section(|| self.compression_type.clone().header("Compressor"))
      .map_err(|report| std::io::Error::new(ErrorKind::Other, report_to_string(&report)))
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self
      .compressor
      .flush()
      .wrap_err_with(|| "While flushing compressed file")
      .with_section(|| {
        self
          .filepath
          .clone()
          .unwrap_or_else(|| "None".to_owned())
          .header("Filename")
      })
      .with_section(|| self.compression_type.clone().header("Compressor"))
      .map_err(|report| std::io::Error::new(ErrorKind::Other, report_to_string(&report)))
  }
}

impl<'w> Drop for Compressor<'w> {
  fn drop(&mut self) {
    self.flush().unwrap();
  }
}
