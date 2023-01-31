use crate::io::csv::CsvStructFileWriter;
use crate::io::file::open_file_or_stdin;
use crate::utils::hash::sha256;
use crate::utils::verbosity::{Verbosity, WarnLevel};
use clap::{Parser, ValueHint};
use eyre::{Report, WrapErr};
use itertools::Itertools;
use log::info;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
/// Find duplicates in GISAID ndjson file
pub struct Args {
  /// Path to GISAID NDJSON file
  ///
  /// Supports the following compression formats: "gz", "bz2", "xz", "zst". If no path provided, the uncompressed input is read from standard input (stdin).
  #[clap(value_hint = ValueHint::FilePath)]
  #[clap(display_order = 1)]
  pub input_ndjson: Option<PathBuf>,

  /// Path to output csv file
  ///
  /// Supports the following compression formats: "gz", "bz2", "xz", "zst". If no path provided, the uncompressed output is written to standard output (stdout).
  #[clap(value_hint = ValueHint::FilePath)]
  #[clap(display_order = 2)]
  #[clap(long, short = 'o')]
  #[clap(value_hint = ValueHint::AnyPath)]
  pub output_csv: Option<String>,

  /// Number of processing jobs. If not specified, all available CPU threads will be used.
  #[clap(global = false, long, short = 'j', default_value_t = num_cpus::get())]
  pub jobs: usize,

  /// Make output more quiet or more verbose
  #[clap(flatten)]
  pub verbosity: Verbosity<WarnLevel>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GisaidEntry {
  pub covv_accession_id: String,
  pub covv_virus_name: String,
  pub covv_collection_date: String,
  pub covv_location: String,
  pub sequence: String,
  #[serde(flatten)]
  pub other: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InternalEntry {
  pub index: usize,
  pub seq_name: String,
  pub seq_hash: String,
  pub accession: String,
  pub location: String,
  pub date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputEntry {
  /// Sequence name
  pub seq_name: String,

  /// Number of occurences of a the sequence name
  pub n_name_occurences: usize,

  // Number of unique hashes of nucleotide sequences this name is associated with
  pub n_unique_seq_hashes: usize,

  // List of unique accessions this name is associated with
  pub accessions: String,

  // List of unique dates this name is associated with
  pub dates: String,

  // List of unique locations this name is associated with
  pub locations: String,

  // List of unique hashes of nucleotide sequences this name is associated with
  pub hashes: String,
}

impl GisaidEntry {
  pub fn from_json_str(s: &str) -> Result<Self, Report> {
    serde_json::from_str::<GisaidEntry>(s).wrap_err("When parsing GISAID entry")
  }
}

pub fn run(args: Args) -> Result<(), Report> {
  std::thread::scope(|s| {
    let (input_sender, input_receiver) = crossbeam_channel::bounded::<(usize, String)>(256);
    let (result_sender, result_receiver) = crossbeam_channel::bounded::<InternalEntry>(256);

    // Line reader thread
    s.spawn(|| {
      let file = open_file_or_stdin(&args.input_ndjson)
        .wrap_err("When opening input file")
        .unwrap();

      file.lines().into_iter().enumerate().for_each(|(index, line)| {
        let line = line.wrap_err_with(|| format!("When reading line '{index}'")).unwrap();
        input_sender
          .send((index, line))
          .wrap_err_with(|| format!("When sending a line {index}"))
          .unwrap();
      });

      drop(input_sender);
    });

    // Parser threads
    for _ in 0..args.jobs {
      let input_receiver = input_receiver.clone();
      let result_sender = result_sender.clone();

      s.spawn(move || {
        let result_sender = result_sender.clone();

        for (index, line) in &input_receiver {
          let entry = GisaidEntry::from_json_str(&line)
            .wrap_err_with(|| format!("When parsing line {index}: {line}"))
            .unwrap();

          result_sender
            .send(InternalEntry {
              index,
              accession: entry.covv_accession_id,
              location: entry.covv_location,
              date: entry.covv_collection_date,
              seq_name: entry.covv_virus_name,
              seq_hash: sha256(&entry.sequence),
            })
            .wrap_err("When sending result")
            .unwrap();

          if (index > 0) && ((index % 1_000_000) == 0) {
            info!("Parsed {index:>8} entries");
          }
        }

        drop(result_sender);
      });
    }

    // Bookkeeping and output thread
    s.spawn(move || {
      let mut bookkeeping = HashMap::<String, Vec<InternalEntry>>::new();

      let mut last_index = 0;
      for (index, entry) in result_receiver.into_iter().enumerate() {
        last_index = index;
        bookkeeping.entry(entry.seq_name.clone()).or_default().push(entry);
      }

      info!("Finished parsing. Total entries: {}", last_index + 1);
      info!("Writing results");

      let mut csv_writer = CsvStructFileWriter::new(args.output_csv.unwrap_or_else(|| "-".to_owned()), b'\t').unwrap();

      let outputs: Vec<OutputEntry> = bookkeeping
        .into_par_iter()
        .filter_map(|(seq_name, entries)| {
          let n_name_occurences = entries.len();

          if n_name_occurences <= 1 {
            return None;
          }

          let hashes = entries
            .iter()
            .map(|entry| entry.seq_hash.as_str())
            .unique()
            .sorted()
            .collect_vec();
          let n_unique_seq_hashes = hashes.len();
          let hashes = hashes.join(";");

          let dates = entries.iter().map(|entry| &entry.date).unique().sorted().join(";");
          let accessions = entries.iter().map(|entry| &entry.accession).unique().sorted().join(";");
          let locations = entries.iter().map(|entry| &entry.location).unique().sorted().join(";");

          Some(OutputEntry {
            seq_name,
            n_name_occurences,
            n_unique_seq_hashes,
            accessions,
            dates,
            locations,
            hashes,
          })
        })
        .collect();

      outputs
        .into_iter()
        .sorted_by_key(|entry| {
          (
            -(entry.n_name_occurences as isize),
            -(entry.n_unique_seq_hashes as isize),
          )
        })
        .try_for_each(|entry| csv_writer.write(&entry))
        .unwrap();
    });
  });

  Ok(())
}
