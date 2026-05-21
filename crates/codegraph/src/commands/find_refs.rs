use crate::cli::FindRefsArgs;
use crate::output::print_json;
use serde::Serialize;

#[derive(Serialize, Default)]
struct Empty(Vec<()>);

pub fn run(args: FindRefsArgs) -> anyhow::Result<()> {
    if args.json {
        print_json(Empty::default())?;
    } else {
        // Non-JSON path will be filled in once we have real hits to print.
    }
    Ok(())
}
