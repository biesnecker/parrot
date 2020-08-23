use crate::tts::{TTSVoice, TTS};
use anyhow::{bail, Result};
use fasthash::XXHasher;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    fs::File,
    hash::{Hash, Hasher},
    path::PathBuf,
};
use structopt::StructOpt;
use tokio::fs::File as AsyncFile;
use tokio::prelude::*;

#[derive(Debug, StructOpt)]
#[structopt(
    no_version,
    about = "Generate Anki cards using a particular AWS Polly voice"
)]
pub struct Opts {
    #[structopt(parse(from_os_str), help = "Source file")]
    pub source: PathBuf,

    #[structopt(parse(from_os_str), help = "Target file")]
    pub target: PathBuf,

    #[structopt(parse(from_os_str), help = "Directory where audio files are written")]
    pub audio_directory: PathBuf,

    #[structopt(short, long, help = "Amazon Polly voice ID")]
    pub voice: String,

    #[structopt(long, help = "Use the neural voice (voice must support it)")]
    pub neural: bool,

    #[structopt(long, help = "TSV instead of CSV")]
    pub tabs: bool,

    #[structopt(long, help = "Overwrite existing files in audio directory")]
    pub force: bool,
}

#[derive(Debug)]
struct WorkItem {
    seq: usize,
    record: csv::StringRecord,
    sentence_hash: u64,
    record_hash: u64,
    output_path: PathBuf,
}

impl WorkItem {
    pub fn new_from_record(seq: usize, record: csv::StringRecord) -> Self {
        let mut hasher = XXHasher::default();
        record[0].hash(&mut hasher);
        let sentence_hash = hasher.finish();
        let mut record_hash = sentence_hash;
        if record.len() > 1 {
            let mut hasher = XXHasher::default();
            for field in record.iter() {
                field.hash(&mut hasher);
            }
            record_hash = hasher.finish();
        }
        let output_path = format!("parrot_{}.mp3", sentence_hash).into();
        WorkItem {
            seq,
            record,
            sentence_hash,
            record_hash,
            output_path,
        }
    }
}

#[derive(Debug)]
struct WorkBundle {
    needs_tts: BTreeMap<u64, String>,
    work_items: Vec<WorkItem>,
}

impl WorkBundle {
    fn new() -> Self {
        WorkBundle {
            needs_tts: BTreeMap::new(),
            work_items: Vec::new(),
        }
    }

    fn add_work_item(&mut self, wi: WorkItem) {
        self.needs_tts
            .entry(wi.sentence_hash)
            .or_insert_with(|| wi.record[0].to_string());
        self.work_items.push(wi);
    }
}

fn get_csv_reader(options: &Opts) -> Result<csv::Reader<File>> {
    let mut rdr_builder = csv::ReaderBuilder::new();
    if options.tabs {
        rdr_builder.delimiter(b'\t');
    }
    let reader = rdr_builder.from_path(&options.source)?;
    Ok(reader)
}

async fn read_source(options: &Opts) -> Result<Vec<csv::StringRecord>> {
    let reader = get_csv_reader(&options)?;
    reader
        .into_records()
        .map(|r| {
            let rc = r?;
            if rc.is_empty() {
                bail!("All rows in the source must have at least one field");
            }
            Ok(rc)
        })
        .collect::<Result<Vec<csv::StringRecord>>>()
}

pub async fn exec(tts: TTS, options: Opts) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut work = WorkBundle::new();
    read_source(&options)
        .await?
        .into_iter()
        .enumerate()
        // Filter out already seen sentences.
        .filter_map(|(seq, record)| {
            let wi = WorkItem::new_from_record(seq, record);
            if (options.force && wi.output_path.exists()) || seen.contains(&wi.record_hash) {
                None
            } else {
                seen.insert(wi.record_hash);
                Some(wi)
            }
        })
        .collect::<Vec<WorkItem>>()
        .into_iter()
        // Collect work items into a WorkBundle
        .fold(&mut work, |acc, wi| {
            acc.add_work_item(wi);
            acc
        });

    let maybe_voice = tts
        .list_voices(None)
        .await?
        .into_iter()
        .filter(|v| {
            if let Some(vid) = &v.id {
                vid.to_lowercase() == options.voice.to_lowercase()
            } else {
                false
            }
        })
        .map(TTSVoice::try_from)
        .collect::<Result<Vec<TTSVoice>>>()?
        .into_iter()
        .find(|v| !options.neural || v.neural);

    if let Some(voice) = maybe_voice {
        let results = tts
            .generate_many(&work.needs_tts, &voice, options.neural)
            .await?;
        // Open the output file for writing.
        let mut wb = csv::WriterBuilder::new();
        if options.tabs {
            wb.delimiter(b'\t');
        }
        let mut writer = wb.from_path(&options.target)?;
        // TODO: headers

        for wi in work.work_items.iter() {
            if let Some(res) = results.get(&wi.sentence_hash) {
                let mut file =
                    AsyncFile::create(options.audio_directory.join(&wi.output_path)).await?;
                file.write_all(res).await?;
                let mut output_row = wi.record.clone();
                let output_path_str =
                    format!("[sound:{}]", wi.output_path.as_os_str().to_string_lossy());
                output_row.push_field(output_path_str.as_str());
                writer.write_record(&output_row)?;
            } else {
                bail!("Couldn't find result for {}", wi.sentence_hash);
            }
        }
        writer.flush()?;
        Ok(())
    } else {
        bail!("Couldn't find voice {}", options.voice);
    }
}
