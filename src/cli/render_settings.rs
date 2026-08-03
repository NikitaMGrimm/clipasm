//! Shared project and command-line render settings.

use clap::{Args, ValueEnum};
use serde::Deserialize;

use clipasm::render::{self, RenderOptions};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Args)]
#[serde(deny_unknown_fields)]
pub(super) struct RenderSettingsPatch {
    /// Override the project's persistent working-artifact cache policy.
    #[arg(long, value_enum, value_name = "MODE")]
    cache: Option<CacheSetting>,
    /// Override which compatible `FFmpeg` primitives share an execution job.
    #[arg(long, value_enum, value_name = "MODE")]
    materialization: Option<MaterializationSetting>,
}

impl RenderSettingsPatch {
    pub(super) fn resolve(project: Option<Self>, command_line: Self) -> RenderOptions {
        let Self {
            cache: project_cache,
            materialization: project_materialization,
        } = project.unwrap_or_default();
        let Self {
            cache: command_line_cache,
            materialization: command_line_materialization,
        } = command_line;
        let defaults = RenderOptions::default();

        RenderOptions::new(
            command_line_cache
                .or(project_cache)
                .map_or(defaults.cache_mode(), CacheSetting::render_mode),
            command_line_materialization
                .or(project_materialization)
                .map_or(
                    defaults.materialization_mode(),
                    MaterializationSetting::render_mode,
                ),
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum MaterializationSetting {
    All,
    Fused,
}

impl MaterializationSetting {
    const fn render_mode(self) -> render::MaterializationMode {
        match self {
            Self::All => render::MaterializationMode::All,
            Self::Fused => render::MaterializationMode::Fused,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum CacheSetting {
    Persistent,
    None,
}

impl CacheSetting {
    const fn render_mode(self) -> render::CacheMode {
        match self {
            Self::Persistent => render::CacheMode::Persistent,
            Self::None => render::CacheMode::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    #[test]
    fn resolution_applies_defaults_then_project_then_command_line() {
        let project = RenderSettingsPatch {
            cache: Some(CacheSetting::None),
            materialization: Some(MaterializationSetting::Fused),
        };
        for (name, project, command_line, cache, materialization) in [
            (
                "defaults",
                None,
                RenderSettingsPatch::default(),
                render::CacheMode::Persistent,
                render::MaterializationMode::All,
            ),
            (
                "project",
                Some(project),
                RenderSettingsPatch::default(),
                render::CacheMode::None,
                render::MaterializationMode::Fused,
            ),
            (
                "materialization override",
                Some(project),
                RenderSettingsPatch {
                    cache: None,
                    materialization: Some(MaterializationSetting::All),
                },
                render::CacheMode::None,
                render::MaterializationMode::All,
            ),
            (
                "cache override",
                Some(project),
                RenderSettingsPatch {
                    cache: Some(CacheSetting::Persistent),
                    materialization: None,
                },
                render::CacheMode::Persistent,
                render::MaterializationMode::Fused,
            ),
        ] {
            let options = RenderSettingsPatch::resolve(project, command_line);
            assert_eq!(options.cache_mode(), cache, "{name}");
            assert_eq!(options.materialization_mode(), materialization, "{name}");
        }
    }

    #[test]
    fn project_parsing_preserves_omitted_settings() {
        let empty: RenderSettingsPatch = toml::from_str("").expect("empty render settings");
        assert_eq!(empty, RenderSettingsPatch::default());

        let cache: RenderSettingsPatch =
            toml::from_str("cache = \"none\"\n").expect("cache setting");
        assert_eq!(cache.cache, Some(CacheSetting::None));
        assert_eq!(cache.materialization, None);

        let materialization: RenderSettingsPatch =
            toml::from_str("materialization = \"fused\"\n").expect("materialization setting");
        assert_eq!(materialization.cache, None);
        assert_eq!(
            materialization.materialization,
            Some(MaterializationSetting::Fused)
        );
    }

    #[test]
    fn command_line_parsing_preserves_omitted_settings() {
        let cli =
            Cli::try_parse_from(["clipasm", "render", "source.clipasm"]).expect("render command");
        let Command::Render { settings, .. } = cli.command else {
            panic!("expected render command");
        };
        assert_eq!(settings, RenderSettingsPatch::default());

        let cli = Cli::try_parse_from([
            "clipasm",
            "render",
            "source.clipasm",
            "--cache",
            "persistent",
            "--materialization",
            "all",
        ])
        .expect("render command with settings");
        let Command::Render { settings, .. } = cli.command else {
            panic!("expected render command");
        };
        assert_eq!(settings.cache, Some(CacheSetting::Persistent));
        assert_eq!(settings.materialization, Some(MaterializationSetting::All));
    }
}
