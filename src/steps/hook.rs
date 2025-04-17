use std::ffi::OsStr;
use std::path::Path;

use crate::error::CliError;
use crate::ops::cmd;
use crate::ops::git;
use crate::ops::replace::{NOW, Template};
use crate::steps::plan;

/// Run pre-release hooks
#[derive(Debug, Clone, clap::Args)]
pub struct HookStep {
    #[command(flatten)]
    manifest: clap_cargo::Manifest,

    #[command(flatten)]
    workspace: clap_cargo::Workspace,

    /// Process all packages whose current version is unpublished
    #[arg(long)]
    unpublished: bool,

    /// Custom config file
    #[arg(short, long = "config", value_name = "PATH")]
    custom_config: Option<std::path::PathBuf>,

    /// Ignore implicit configuration files.
    #[arg(long)]
    isolated: bool,

    /// Unstable options
    #[arg(short = 'Z', value_name = "FEATURE")]
    z: Vec<crate::config::UnstableValues>,

    /// Comma-separated globs of branch names a release can happen from
    #[arg(long, value_delimiter = ',')]
    allow_branch: Option<Vec<String>>,

    /// Actually perform a release. Dry-run mode is the default
    #[arg(short = 'x', long)]
    execute: bool,

    #[arg(short = 'n', long, conflicts_with = "execute", hide = true)]
    dry_run: bool,

    /// Exclude unchanged packages
    #[arg(long)]
    exclude_unchanged: bool,

    /// Skip release confirmation and version preview
    #[arg(long)]
    no_confirm: bool,
}

impl HookStep {
    pub fn run(&self) -> Result<(), CliError> {
        git::git_version()?;
        let mut index = crate::ops::index::CratesIoIndex::new();

        if self.dry_run {
            let _ =
                crate::ops::shell::warn("`--dry-run` is superfluous, dry-run is done by default");
        }

        let ws_meta = self
            .manifest
            .metadata()
            // When evaluating dependency ordering, we need to consider optional dependencies
            .features(cargo_metadata::CargoOpt::AllFeatures)
            .exec()?;
        let config = self.to_config();
        let ws_config = crate::config::load_workspace_config(&config, &ws_meta)?;
        let mut pkgs = plan::load(&config, &ws_meta)?;

        let (_selected_pkgs, excluded_pkgs) = self.workspace.partition_packages(&ws_meta);
        for excluded_pkg in excluded_pkgs {
            let Some(pkg) = pkgs.get_mut(&excluded_pkg.id) else {
                // Either not in workspace or marked as `release = false`.
                continue;
            };
            if !pkg.config.release() {
                continue;
            }

            let crate_name = pkg.meta.name.as_str();
            let explicitly_excluded = self.workspace.exclude.contains(&excluded_pkg.name);
            // 1. Don't show this message if already not releasing in config
            // 2. Still respect `--exclude`
            if pkg.config.release()
                && pkg.config.publish()
                && self.unpublished
                && !explicitly_excluded
            {
                let version = &pkg.initial_version;
                if !crate::ops::cargo::is_published(
                    &mut index,
                    pkg.config.registry(),
                    crate_name,
                    &version.full_version_string,
                    pkg.config.certs_source(),
                ) {
                    log::debug!(
                        "enabled {}, v{} is unpublished",
                        crate_name,
                        version.full_version_string
                    );
                    continue;
                }
            }

            pkg.config.pre_release_replacements = Some(vec![]);
            pkg.config.release = Some(false);
        }

        let pkgs = plan::plan(pkgs)?;

        let (selected_pkgs, _excluded_pkgs): (Vec<_>, Vec<_>) = pkgs
            .into_iter()
            .map(|(_, pkg)| pkg)
            .partition(|p| p.config.release());
        if selected_pkgs.is_empty() {
            let _ = crate::ops::shell::error("no packages selected");
            return Err(2.into());
        }

        let dry_run = !self.execute;
        let mut failed = false;

        // STEP 0: Help the user make the right decisions.
        failed |= !super::verify_git_is_clean(
            ws_meta.workspace_root.as_std_path(),
            dry_run,
            log::Level::Warn,
        )?;

        let selected_pkgs = super::detect_changed(&ws_meta, &selected_pkgs, self.exclude_unchanged)?;

        failed |= !super::verify_git_branch(
            ws_meta.workspace_root.as_std_path(),
            &ws_config,
            dry_run,
            log::Level::Warn,
        )?;

        failed |= !super::verify_if_behind(
            ws_meta.workspace_root.as_std_path(),
            &ws_config,
            dry_run,
            log::Level::Warn,
        )?;

        // STEP 1: Release Confirmation
        super::confirm("Bump", &selected_pkgs, self.no_confirm, dry_run)?;

        // STEP 2: update current version, save and commit
        for pkg in &selected_pkgs {
            hook(&ws_meta, pkg, dry_run)?;
        }

        super::finish(failed, dry_run)
    }

    fn to_config(&self) -> crate::config::ConfigArgs {
        crate::config::ConfigArgs {
            custom_config: self.custom_config.clone(),
            isolated: self.isolated,
            z: self.z.clone(),
            allow_branch: self.allow_branch.clone(),
            ..Default::default()
        }
    }
}

pub fn hook(
    ws_meta: &cargo_metadata::Metadata,
    pkg: &plan::PackageRelease,
    dry_run: bool,
) -> Result<(), CliError> {
    if let Some(pre_rel_hook) = pkg.config.pre_release_hook() {
        let cwd = &pkg.package_root;
        let crate_name = pkg.meta.name.as_str();
        let version = pkg.planned_version.as_ref().unwrap_or(&pkg.initial_version);
        let prev_version_var = pkg.initial_version.bare_version_string.as_str();
        let prev_metadata_var = pkg.initial_version.full_version.build.as_str();
        let version_var = version.bare_version_string.as_str();
        let metadata_var = version.full_version.build.as_str();
        let template = Template {
            prev_version: Some(prev_version_var),
            prev_metadata: Some(prev_metadata_var),
            version: Some(version_var),
            metadata: Some(metadata_var),
            crate_name: Some(crate_name),
            date: Some(NOW.as_str()),
            tag_name: pkg.planned_tag.as_deref(),
            ..Default::default()
        };
        let pre_rel_hook = pre_rel_hook
            .args()
            .into_iter()
            .map(|arg| template.render(arg))
            .collect::<Vec<_>>();
        log::debug!("calling pre-release hook: {:?}", pre_rel_hook);
        let envs = maplit::btreemap! {
            OsStr::new("PREV_VERSION") => prev_version_var.as_ref(),
            OsStr::new("PREV_METADATA") => prev_metadata_var.as_ref(),
            OsStr::new("NEW_VERSION") => version_var.as_ref(),
            OsStr::new("NEW_METADATA") => metadata_var.as_ref(),
            OsStr::new("DRY_RUN") => OsStr::new(if dry_run { "true" } else { "false" }),
            OsStr::new("CRATE_NAME") => OsStr::new(crate_name),
            OsStr::new("WORKSPACE_ROOT") => ws_meta.workspace_root.as_os_str(),
            OsStr::new("CRATE_ROOT") => pkg.manifest_path.parent().unwrap_or_else(|| Path::new(".")).as_os_str(),
        };
        // we use dry_run environmental variable to run the script
        // so here we set dry_run=false and always execute the command.
        if !cmd::call_with_env(pre_rel_hook, envs, cwd, false)? {
            let _ = crate::ops::shell::error(format!(
                "release of {crate_name} aborted by non-zero return of prerelease hook."
            ));
            return Err(101.into());
        }
    }

    Ok(())
}
