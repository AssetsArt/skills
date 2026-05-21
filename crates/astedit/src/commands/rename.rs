use crate::cli::RenameArgs;
use crate::output::print_json;
use crate::serialize::RenameData;

pub fn run(args: RenameArgs) -> anyhow::Result<i32> {
    // PR 2 implementation will fill this out in Tasks 8 onward. For now,
    // emit an empty dry-run envelope so the CLI is wireable.
    let data = RenameData {
        subcommand: "rename",
        dry_run: !args.apply,
        needs_anchor: None,
        candidates: None,
        applied: Some(vec![]),
        skipped: Some(vec![]),
        errors: Some(vec![]),
    };
    if args.json {
        print_json(data)?;
    }
    Ok(0)
}
