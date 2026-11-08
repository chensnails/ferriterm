pub fn output(id: &str) -> String {
    format!("terminal://{id}/output")
}

pub fn closed(id: &str) -> String {
    format!("terminal://{id}/closed")
}
