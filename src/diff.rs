use std::path::Path;

use similar::TextDiff;

pub(crate) fn unified_diff(old: &str, new: &str, path: &Path, new_description: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let path = path.display();
    diff.unified_diff()
        .header(
            &format!("{path}\toriginal"),
            &format!("{path}\t{new_description}"),
        )
        .to_string()
}
