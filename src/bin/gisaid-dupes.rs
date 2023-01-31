use clap::Parser;
use ctor::ctor;
use eyre::Report;
use gisaid_dupes::dupes::run::{run, Args};
use gisaid_dupes::utils::global_init::setup_logger;

#[ctor]
fn init() {
  color_eyre::install().expect("color_eyre initialization failed");
}

fn main() -> Result<(), Report> {
  let args = Args::parse();
  setup_logger(args.verbosity.get_filter_level());
  run(args)
}
