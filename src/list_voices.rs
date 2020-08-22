use crate::tts::{TTSVoice, TTS};
use anyhow::Result;
use itertools::Itertools;
use std::convert::TryFrom;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(no_version, about = "List all available AWS Polly voices")]
pub struct Opts {
    #[structopt(short, long, help = "Only show voices for this language")]
    pub language: Option<String>,
}

pub async fn exec(tts: TTS, options: Opts) -> Result<()> {
    let voices = tts
        .list_voices(options.language)
        .await?
        .into_iter()
        .map(TTSVoice::try_from)
        .collect::<Result<Vec<TTSVoice>>>()?
        .into_iter()
        .map(|v| (v.language.clone(), v))
        .into_group_map();

    let mut keys: Vec<_> = voices.keys().collect();
    keys.sort();
    for key in keys.into_iter() {
        if let Some(voices) = voices.get(key) {
            println!("\n===== {}\n", key);
            for voice in voices {
                let id = voice.id.as_str();
                let gender = match voice.gender.to_lowercase().as_str() {
                    "male" => "♂",
                    "female" => "♀",
                    _ => "?",
                };
                let neural = match voice.neural {
                    true => "supports neural",
                    false => "standard only",
                };
                println!("{} {:15} ({})", gender, id, neural);
            }
        }
    }
    Ok(())
}
