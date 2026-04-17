use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ResponseEnvelope<T: Serialize> {
    pub query: String,
    pub results: T,
    pub truncated: bool,
    pub token_estimate: usize,
    pub next_actions: Vec<String>,
}

impl<T: Serialize> ResponseEnvelope<T> {
    pub fn new(query: impl Into<String>, results: T) -> Self {
        let json = serde_json::to_string(&results).unwrap_or_default();
        let token_estimate = json.len() / 4;
        Self {
            query: query.into(),
            results,
            truncated: false,
            token_estimate,
            next_actions: vec![],
        }
    }

    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated;
        self
    }

    pub fn print_json(&self) {
        println!("{}", serde_json::to_string_pretty(self).unwrap_or_default());
    }
}

pub fn print_json<T: Serialize>(query: impl Into<String>, results: T) {
    let envelope = ResponseEnvelope::new(query, results);
    envelope.print_json();
}

pub fn print_json_truncated<T: Serialize>(query: impl Into<String>, results: T, truncated: bool) {
    let envelope = ResponseEnvelope::new(query, results).with_truncated(truncated);
    envelope.print_json();
}
