use crate::cli::RewriteArgs;
use crate::output::print_json;
use crate::serialize::RewriteData;

pub fn run(args: RewriteArgs) -> anyhow::Result<i32> {
    // Placeholder implementation. Task 5 onward fills this out.
    let data = RewriteData {
        subcommand: "rewrite",
        dry_run: !args.apply,
        applied: Some(vec![]),
        errors: Some(vec![]),
    };
    if args.json {
        print_json(data)?;
    }
    Ok(0)
}
