wit_bindgen::generate!({
    path: "../../../../../wit/cli",
    world: "pipe-plugin",
});

use exports::wacli::cli::pipe::Guest;
use wacli::cli::types::{PipeError, PipeMeta};

struct TablePipe;

impl Guest for TablePipe {
    fn meta() -> PipeMeta {
        PipeMeta {
            name: "format/table".to_string(),
            summary: "Uppercase formatter (test)".to_string(),
            input_types: vec!["text/plain".to_string()],
            output_type: "text/plain".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    fn process(input: Vec<u8>, _options: Vec<String>) -> Result<Vec<u8>, PipeError> {
        let s = String::from_utf8(input).map_err(|e| PipeError::ParseError(e.to_string()))?;
        Ok(s.to_uppercase().into_bytes())
    }
}

export!(TablePipe);
