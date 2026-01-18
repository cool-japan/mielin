//! Operation result data structures

use crate::output::MultiFormatDisplay;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl MultiFormatDisplay for OperationResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        let status_color = if self.success {
            Color::Green
        } else {
            Color::Red
        };
        let status = if self.success { "Success" } else { "Failed" };

        table.add_row(vec![
            Cell::new("Status").fg(Color::Cyan),
            Cell::new(status).fg(status_color),
        ]);
        table.add_row(vec![
            Cell::new("Message").fg(Color::Cyan),
            Cell::new(&self.message),
        ]);
        if let Some(ref id) = self.id {
            table.add_row(vec![Cell::new("ID").fg(Color::Cyan), Cell::new(id)]);
        }

        table
    }

    fn to_quiet(&self) -> String {
        if self.success {
            self.id.clone().unwrap_or_default()
        } else {
            String::new()
        }
    }
}
