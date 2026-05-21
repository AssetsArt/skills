#![allow(dead_code)]

use serde::Serialize;

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema_version: u32,
    data: T,
}

pub fn print_json<T: Serialize>(data: T) -> anyhow::Result<()> {
    let env = Envelope {
        schema_version: 1,
        data,
    };
    println!("{}", serde_json::to_string(&env)?);
    Ok(())
}
