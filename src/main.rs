use crate::tts::TTS;
use anyhow::Result;
use structopt::StructOpt;

mod generate;
mod list_voices;
mod tts;

#[derive(Debug, StructOpt)]
#[structopt(name = "parrot", about = "Generate Anki cards with AWS Polly TTS")]
enum Command {
    Generate(generate::Opts),
    ListVoices(list_voices::Opts),
}

async fn main_impl(options: Command) -> Result<()> {
    let tts = TTS::new()?;
    match options {
        Command::Generate(opts) => generate::exec(tts, opts).await?,
        Command::ListVoices(opts) => list_voices::exec(tts, opts).await?,
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let command = Command::from_args();
    if let Err(e) = main_impl(command).await {
        println!("{}", e);
    }
}
