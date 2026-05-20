use serde::Serialize;

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema_version: u32,
    data: T,
}

/// Print `data` wrapped in the v1 envelope `{schema_version:1, data:...}`.
/// Every subcommand that produces JSON routes through this helper.
pub fn print_json<T: Serialize>(data: T) -> anyhow::Result<()> {
    let env = Envelope { schema_version: 1, data };
    println!("{}", serde_json::to_string(&env)?);
    Ok(())
}
