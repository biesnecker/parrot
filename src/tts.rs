use anyhow::{bail, Result};
use bytes::Bytes;
use futures::future::join_all;
use rusoto_core::{credential, request, Region};
use rusoto_polly::{DescribeVoicesInput, Polly, PollyClient, SynthesizeSpeechInput, Voice};
use std::{collections::BTreeMap, convert::TryFrom};

pub struct TTS {
    polly_client: PollyClient,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct TTSVoice {
    pub id: String,
    pub gender: String,
    pub language: String,
    pub code: String,
    pub neural: bool,
}

impl TTSVoice {
    fn new_from_voice(v: Voice) -> Option<Self> {
        let id = v.id?;
        let gender = v.gender?;
        let language = v.language_name?;
        let code = v.language_code?;
        let eng = match v.supported_engines {
            None => vec![],
            Some(engs) => engs,
        };
        let neural = eng.iter().any(|e| e.to_lowercase() == "neural");
        Some(Self {
            id,
            gender,
            language,
            code,
            neural,
        })
    }
}

impl TryFrom<Voice> for TTSVoice {
    type Error = anyhow::Error;

    fn try_from(v: Voice) -> Result<Self, Self::Error> {
        if let Some(ttsv) = TTSVoice::new_from_voice(v) {
            Ok(ttsv)
        } else {
            bail!("Unable to convert Polly Voice to TTSVoice");
        }
    }
}

impl TTS {
    pub fn new() -> Result<TTS> {
        let dispatcher = request::HttpClient::new()?;
        let creds = credential::ChainProvider::new();
        let client = PollyClient::new_with(dispatcher, creds, Region::default());
        Ok(TTS {
            polly_client: client,
        })
    }

    pub async fn list_voices(&self, language: Option<String>) -> Result<Vec<Voice>> {
        let input = DescribeVoicesInput {
            engine: None,
            include_additional_language_codes: Some(false),
            language_code: language,
            next_token: None,
        };
        let request_result = self.polly_client.describe_voices(input).await?;
        if let Some(polly_voices) = request_result.voices {
            Ok(polly_voices)
        } else {
            bail!("No voices returned");
        }
    }

    pub async fn generate_many(
        &self,
        tasks: &BTreeMap<u64, String>,
        voice: &TTSVoice,
        use_neural: bool,
    ) -> Result<BTreeMap<u64, Bytes>> {
        Ok(join_all(
            tasks
                .iter()
                .map(|(k, v)| self.generate_one(*k, v.clone(), &voice, use_neural)),
        )
        .await
        .into_iter()
        .collect::<Result<Vec<(u64, Bytes)>>>()?
        .into_iter()
        .collect::<BTreeMap<u64, Bytes>>())
    }

    pub async fn generate_one(
        &self,
        key: u64,
        sentence: String,
        voice: &TTSVoice,
        use_neural: bool,
    ) -> Result<(u64, Bytes)> {
        let input = SynthesizeSpeechInput {
            engine: if use_neural {
                Some("neural".to_string())
            } else {
                Some("standard".to_string())
            },
            language_code: None,
            lexicon_names: None,
            output_format: "mp3".to_string(),
            sample_rate: None,
            speech_mark_types: None,
            text: sentence.clone(),
            text_type: None,
            voice_id: voice.id.clone(),
        };
        let result = self.polly_client.synthesize_speech(input).await?;
        match result.audio_stream {
            Some(bytes) => Ok((key, bytes)),
            None => bail!("Unable to get bytes from result."),
        }
    }
}
