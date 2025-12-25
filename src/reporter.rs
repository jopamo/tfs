use crate::events::Event;

/// Reporter aggregates events and produces human or JSON output.
pub struct Reporter {
    events: Vec<Event>,
    json_mode: bool,
}

impl Reporter {
    pub fn new(json_mode: bool) -> Self {
        Self {
            events: Vec::new(),
            json_mode,
        }
    }

    pub fn record(&mut self, event: Event) {
        if self.json_mode {
            // Emit JSON line to stdout
            if let Ok(line) = serde_json::to_string(&event) {
                println!("{}", line);
            }
        }
        self.events.push(event);
    }

    pub fn summary(&self) -> String {
        // TODO: produce human-readable summary
        format!("{} events recorded", self.events.len())
    }
}