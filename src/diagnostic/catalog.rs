//! Canonical built-in diagnostic identifiers and reference facts.
//!
//! Dynamic messages and source locations remain at construction sites. This
//! module owns only stable, context-free facts and is not consumed by semantic,
//! format, protocol, or cache identity code.

use std::fmt;

/// A user-oriented stage in which a built-in diagnostic can occur.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum DiagnosticCategory {
    /// Command-line arguments, project creation, and inspection output.
    CommandLineAndProjects,
    /// Lexing, parsing, configuration, and source-file structure.
    ParsingAndSource,
    /// Imports, declarations, names, and authored external definitions.
    ImportsAndDeclarations,
    /// Call arguments, value types, generic inference, and stack binding.
    TypesAndStack,
    /// Pure compilation, graph construction, and timeline evaluation.
    CompilationAndTimelines,
    /// Asset validation, media probing, and required host tools.
    PreflightAndMedia,
    /// External-program resolution, execution, and protocol handling.
    ExternalPrograms,
    /// Render execution, manifests, output validation, and publication.
    RenderingAndPublication,
    /// Cache coordination and filesystem access outside publication.
    CacheAndFilesystem,
    /// Browser asset preparation and browser rendering.
    BrowserRuntime,
    /// Invariants whose failure normally indicates a `ClipAsm` defect.
    InternalContractFailures,
}

impl DiagnosticCategory {
    /// Categories in documentation order.
    pub const ALL: [Self; 11] = [
        Self::CommandLineAndProjects,
        Self::ParsingAndSource,
        Self::ImportsAndDeclarations,
        Self::TypesAndStack,
        Self::CompilationAndTimelines,
        Self::PreflightAndMedia,
        Self::ExternalPrograms,
        Self::RenderingAndPublication,
        Self::CacheAndFilesystem,
        Self::BrowserRuntime,
        Self::InternalContractFailures,
    ];

    /// Return the user-facing category name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CommandLineAndProjects => "Command line and project creation",
            Self::ParsingAndSource => "Parsing and source structure",
            Self::ImportsAndDeclarations => "Imports and declarations",
            Self::TypesAndStack => "Arguments, types, and stack binding",
            Self::CompilationAndTimelines => "Compilation and timeline evaluation",
            Self::PreflightAndMedia => "Preflight and media",
            Self::ExternalPrograms => "External programs",
            Self::RenderingAndPublication => "Rendering and publication",
            Self::CacheAndFilesystem => "Cache and filesystem",
            Self::BrowserRuntime => "Browser runtime",
            Self::InternalContractFailures => "Internal contract failures",
        }
    }

    /// Return the generated section identifier.
    #[must_use]
    pub const fn route(self) -> &'static str {
        match self {
            Self::CommandLineAndProjects => "command-line-and-projects",
            Self::ParsingAndSource => "parsing-and-source",
            Self::ImportsAndDeclarations => "imports-and-declarations",
            Self::TypesAndStack => "types-and-stack",
            Self::CompilationAndTimelines => "compilation-and-timelines",
            Self::PreflightAndMedia => "preflight-and-media",
            Self::ExternalPrograms => "external-programs",
            Self::RenderingAndPublication => "rendering-and-publication",
            Self::CacheAndFilesystem => "cache-and-filesystem",
            Self::BrowserRuntime => "browser",
            Self::InternalContractFailures => "internal",
        }
    }

    /// Return the standalone diagnostics route.
    #[must_use]
    pub const fn documentation_route(self) -> &'static str {
        "diagnostics/index.html"
    }

    const fn related_links(self) -> &'static [RelatedReference] {
        match self {
            Self::CommandLineAndProjects => &COMMAND_LINE_LINKS,
            Self::ParsingAndSource => &PARSING_LINKS,
            Self::ImportsAndDeclarations => &IMPORT_LINKS,
            Self::TypesAndStack => &TYPE_LINKS,
            Self::CompilationAndTimelines => &TIMELINE_LINKS,
            Self::PreflightAndMedia
            | Self::ExternalPrograms
            | Self::CacheAndFilesystem
            | Self::InternalContractFailures => &TROUBLESHOOTING_LINKS,
            Self::RenderingAndPublication => &PUBLICATION_LINKS,
            Self::BrowserRuntime => &[],
        }
    }

    /// Return common causes appropriate to the category.
    #[must_use]
    pub const fn common_causes(self) -> &'static [&'static str] {
        match self {
            Self::CommandLineAndProjects => &[
                "A command argument, destination, or project path is invalid.",
                "An existing file conflicts with the requested operation.",
            ],
            Self::ParsingAndSource => &[
                "The source does not follow the ClipAsm grammar or file structure.",
                "A literal, configuration value, or version declaration is malformed.",
            ],
            Self::ImportsAndDeclarations => &[
                "An import, declaration, alias, or name is missing or conflicts with another.",
                "A referenced source file does not provide the expected declaration.",
            ],
            Self::TypesAndStack => &[
                "A call supplies the wrong arguments, types, or stack values.",
                "ClipAsm cannot infer one unambiguous value type from the available context.",
            ],
            Self::CompilationAndTimelines => &[
                "Authored operations produce an invalid graph, range, or timeline placement.",
                "Exact frame, sample, duration, or output constraints cannot be satisfied.",
            ],
            Self::PreflightAndMedia => &[
                "An asset is missing, unsupported, unstable, or inconsistent with its declaration.",
                "A required media tool is unavailable or cannot inspect the input.",
            ],
            Self::ExternalPrograms => &[
                "An external executable is missing, changed, or failed.",
                "The external program did not follow ClipAsm's request and response protocol.",
            ],
            Self::RenderingAndPublication => &[
                "Rendering failed or produced an artifact that violates the output contract.",
                "The destination cannot safely accept the staged artifact or manifest.",
            ],
            Self::CacheAndFilesystem => &[
                "A required path cannot be resolved, read, written, hashed, or locked.",
                "Another process or a filesystem policy is temporarily blocking access.",
            ],
            Self::BrowserRuntime => &[
                "Browser asset facts do not match the prepared plan.",
                "The browser runtime cannot decode, represent, or render the requested operation.",
            ],
            Self::InternalContractFailures => &[
                "ClipAsm reached a state that its owning phase should have prevented.",
                "User input may have exposed a ClipAsm implementation defect.",
            ],
        }
    }

    /// Return recommended actions appropriate to the category.
    #[must_use]
    pub const fn recommended_actions(self) -> &'static [&'static str] {
        match self {
            Self::CommandLineAndProjects => &[
                "Check the command help, supplied path, and destination.",
                "Use the original diagnostic location and notes to correct the conflicting input.",
            ],
            Self::ParsingAndSource => &[
                "Inspect the highlighted source and the surrounding delimiters or declarations.",
                "Compare the construct with the language reference, then parse again.",
            ],
            Self::ImportsAndDeclarations => &[
                "Check import paths, aliases, declaration names, and exported outputs.",
                "Resolve duplicate or cyclic declarations before compiling again.",
            ],
            Self::TypesAndStack => &[
                "Compare the call with its program signature and inspect the current stack.",
                "Correct argument names, counts, types, or explicit type information.",
            ],
            Self::CompilationAndTimelines => &[
                "Inspect the ranges, placements, and values named by the original diagnostic.",
                "Change the source so graph and exact timeline constraints are satisfiable.",
            ],
            Self::PreflightAndMedia => &[
                "Verify asset paths and declared media facts.",
                "Install or repair the named tool, or stabilize the asset, before retrying.",
            ],
            Self::ExternalPrograms => &[
                "Run the external executable independently and verify its configured path.",
                "Check its ClipAsm protocol output without publishing private inputs.",
            ],
            Self::RenderingAndPublication => &[
                "Preserve the original diagnostic and inspect the staged output or destination.",
                "Correct the render environment or destination before retrying publication.",
            ],
            Self::CacheAndFilesystem => &[
                "Check permissions, free space, path validity, and competing ClipAsm processes.",
                "Retry only after the filesystem or lock condition has changed.",
            ],
            Self::BrowserRuntime => &[
                "Rebuild the prepared browser inputs and verify supplied asset facts.",
                "Check browser support and preserve the failing plan details for diagnosis.",
            ],
            Self::InternalContractFailures => &[
                "Preserve the diagnostic, ClipAsm version, reproduction steps, and non-sensitive inputs.",
                "Report the defect; do not delete caches or generated state unless its explanation says to.",
            ],
        }
    }
}

impl fmt::Display for DiagnosticCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Whether repeating an unchanged operation is likely to help.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum RetryGuidance {
    /// Correct authored source before retrying.
    FixSource,
    /// Correct project metadata or project selection before retrying.
    FixProject,
    /// Correct command or call arguments before retrying.
    FixArguments,
    /// Repair assets, tools, permissions, or another environment dependency.
    FixEnvironment,
    /// Retry after an observed asset, tool, executable, or lock changes.
    RetryAfterExternalChange,
    /// A transient operation may succeed when repeated.
    RetryMayHelp,
    /// An unchanged retry is not expected to help.
    RetryWillNotHelp,
    /// Preserve a safe reproduction and report a likely `ClipAsm` defect.
    ReportBug,
}

impl RetryGuidance {
    /// Return concise user-facing retry advice.
    #[must_use]
    pub const fn explanation(self) -> &'static str {
        match self {
            Self::FixSource => "Retry after correcting the source.",
            Self::FixProject => "Retry after correcting the project manifest or project selection.",
            Self::FixArguments => "Retry after correcting the command or call arguments.",
            Self::FixEnvironment => "Retry after repairing the environment or media inputs.",
            Self::RetryAfterExternalChange => {
                "Retry after the external file, tool, process, or lock state changes."
            }
            Self::RetryMayHelp => "Retrying may help if the failure was transient.",
            Self::RetryWillNotHelp => "Retrying without a relevant change will not help.",
            Self::ReportBug => "Preserve a safe reproduction and report a likely ClipAsm defect.",
        }
    }
}

/// A related generated-book page that can help resolve a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelatedReference {
    label: &'static str,
    documentation_route: &'static str,
}

impl RelatedReference {
    const fn new(label: &'static str, documentation_route: &'static str) -> Self {
        Self {
            label,
            documentation_route,
        }
    }

    /// Return the concise link label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.label
    }

    /// Return the book-root-relative HTML route.
    #[must_use]
    pub const fn documentation_route(self) -> &'static str {
        self.documentation_route
    }
}

static COMMAND_LINE_LINKS: [RelatedReference; 1] = [RelatedReference::new(
    "Command-line reference",
    "reference/cli.html",
)];
static PARSING_LINKS: [RelatedReference; 1] = [RelatedReference::new(
    "Language reference",
    "reference/language/index.html",
)];
static IMPORT_LINKS: [RelatedReference; 1] = [RelatedReference::new(
    "Imports and external programs",
    "reference/language/imports-and-external-programs.html",
)];
static TYPE_LINKS: [RelatedReference; 1] = [RelatedReference::new(
    "Stack binding",
    "reference/language/stack-binding.html",
)];
static TIMELINE_LINKS: [RelatedReference; 1] = [RelatedReference::new(
    "Timeline selectors",
    "reference/language/timeline-selectors.html",
)];
static TROUBLESHOOTING_LINKS: [RelatedReference; 1] = [RelatedReference::new(
    "Troubleshooting",
    "guides/troubleshooting.html",
)];
static PUBLICATION_LINKS: [RelatedReference; 2] = [
    RelatedReference::new("Command-line reference", "reference/cli.html"),
    RelatedReference::new("Troubleshooting", "guides/troubleshooting.html"),
];

/// A sanitized, immutable built-in diagnostic reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticReference {
    diagnostic: BuiltinDiagnostic,
    code: &'static str,
    title: &'static str,
    category: DiagnosticCategory,
    summary: &'static str,
    common_causes: &'static [&'static str],
    recommended_actions: &'static [&'static str],
    specific_common_causes: Option<&'static [&'static str]>,
    specific_recommended_actions: Option<&'static [&'static str]>,
    related_links: &'static [RelatedReference],
    retry_guidance: RetryGuidance,
}

impl DiagnosticReference {
    /// Return the typed built-in identifier.
    #[must_use]
    pub const fn diagnostic(self) -> BuiltinDiagnostic {
        self.diagnostic
    }

    /// Return the machine-readable code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }

    /// Return the concise title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        self.title
    }

    /// Return the user-oriented category.
    #[must_use]
    pub const fn category(self) -> DiagnosticCategory {
        self.category
    }

    /// Return the context-free explanation.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        self.summary
    }

    /// Return the context-free explanation.
    #[must_use]
    pub const fn explanation(self) -> &'static str {
        self.summary
    }

    /// Return common causes for this diagnostic's category.
    #[must_use]
    pub const fn common_causes(self) -> &'static [&'static str] {
        match self.specific_common_causes {
            Some(causes) => causes,
            None => self.common_causes,
        }
    }

    /// Return recommended actions for this diagnostic's category.
    #[must_use]
    pub const fn recommended_actions(self) -> &'static [&'static str] {
        match self.specific_recommended_actions {
            Some(actions) => actions,
            None => self.recommended_actions,
        }
    }

    /// Return recommended actions appropriate to this diagnostic.
    #[must_use]
    pub const fn actions(self) -> &'static [&'static str] {
        self.recommended_actions()
    }

    /// Return related guide and reference pages.
    #[must_use]
    pub const fn related_links(self) -> &'static [RelatedReference] {
        self.related_links
    }

    /// Return the modeled retry guidance.
    #[must_use]
    pub const fn retry_guidance(self) -> RetryGuidance {
        self.retry_guidance
    }

    /// Return the standalone diagnostics route.
    #[must_use]
    pub const fn category_documentation_route(self) -> &'static str {
        self.category.documentation_route()
    }

    /// Return the fragment identifier used for this code.
    #[must_use]
    pub fn documentation_anchor(self) -> String {
        self.code.to_ascii_lowercase()
    }

    /// Return the site-root-relative route including the code fragment.
    #[must_use]
    pub fn documentation_route(self) -> String {
        format!(
            "{}#{}",
            self.category_documentation_route(),
            self.documentation_anchor()
        )
    }

    /// Return the full hosted diagnostics URL for this code.
    #[must_use]
    pub fn documentation_url(self) -> String {
        format!(
            "https://nikitamgrimm.github.io/clipasm/{}#{}",
            self.category_documentation_route(),
            self.documentation_anchor(),
        )
    }
}

macro_rules! diagnostic_catalog {
    (
        $(
            $variant:ident => {
                code: $code:literal,
                title: $title:literal,
                category: $category:ident,
                retry: $retry:ident,
                summary: $summary:literal,
            };
        )+
    ) => {
        /// A typed identifier for a diagnostic emitted by `ClipAsm` itself.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[non_exhaustive]
        pub enum BuiltinDiagnostic {
            $(
                #[doc = concat!("Typed identifier for `", $code, "`.")]
                $variant,
            )+
        }

        impl BuiltinDiagnostic {
            /// Return the machine-readable code.
            #[must_use]
            pub const fn code(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)+
                }
            }

            /// Return the immutable public reference facts.
            #[must_use]
            pub fn reference(self) -> &'static DiagnosticReference {
                reference(self.code()).expect("every typed diagnostic has a catalog reference")
            }
        }

        impl fmt::Display for BuiltinDiagnostic {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.code())
            }
        }

        static REFERENCES: &[DiagnosticReference] = &[
            $(
                DiagnosticReference {
                    diagnostic: BuiltinDiagnostic::$variant,
                    code: $code,
                    title: $title,
                    category: DiagnosticCategory::$category,
                    summary: $summary,
                    common_causes: DiagnosticCategory::$category.common_causes(),
                    recommended_actions: DiagnosticCategory::$category.recommended_actions(),
                    specific_common_causes: specific_common_causes(BuiltinDiagnostic::$variant),
                    specific_recommended_actions: specific_recommended_actions(
                        BuiltinDiagnostic::$variant,
                    ),
                    related_links: DiagnosticCategory::$category.related_links(),
                    retry_guidance: RetryGuidance::$retry,
                },
            )+
        ];
    };
}

diagnostic_catalog! {
    AmbiguousGenericType => {
        code: "E_AMBIGUOUS_GENERIC_TYPE",
        title: "Ambiguous generic type",
        category: TypesAndStack,
        retry: FixSource,
        summary: "ClipAsm could not determine one concrete Video or Audio type for a generic program call.",
    };
    AmbiguousTimelinePlacement => {
        code: "E_AMBIGUOUS_TIMELINE_PLACEMENT",
        title: "Ambiguous timeline placement",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A timeline selector matched more than one authored placement where exactly one was required.",
    };
    ArtifactContract => {
        code: "E_ARTIFACT_CONTRACT",
        title: "Artifact contract failure",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A rendered artifact violated an invariant that ClipAsm should have established before publication.",
    };
    AssetChanged => {
        code: "E_ASSET_CHANGED",
        title: "Asset changed during preparation",
        category: PreflightAndMedia,
        retry: RetryAfterExternalChange,
        summary: "An input asset changed while ClipAsm was inspecting or preparing it.",
    };
    AudioDurationOverflow => {
        code: "E_AUDIO_DURATION_OVERFLOW",
        title: "Audio duration overflow",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "An exact audio duration or sample calculation exceeded ClipAsm's supported range.",
    };
    BodyNestingDepth => {
        code: "E_BODY_NESTING_DEPTH",
        title: "Body nesting limit exceeded",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "Nested program bodies exceeded the parser's supported structural depth.",
    };
    BodyOutputCount => {
        code: "E_BODY_OUTPUT_COUNT",
        title: "Wrong body output count",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A program body left a different number of stack values than its contract requires.",
    };
    BrowserAssetFacts => {
        code: "E_BROWSER_ASSET_FACTS",
        title: "Invalid browser asset facts",
        category: BrowserRuntime,
        retry: FixEnvironment,
        summary: "Browser-supplied media facts were missing, malformed, or inconsistent with the prepared asset.",
    };
    BrowserAssetHash => {
        code: "E_BROWSER_ASSET_HASH",
        title: "Browser asset hash mismatch",
        category: BrowserRuntime,
        retry: RetryAfterExternalChange,
        summary: "A browser asset's content hash did not match the hash recorded during preparation.",
    };
    BrowserAssetPath => {
        code: "E_BROWSER_ASSET_PATH",
        title: "Invalid browser asset path",
        category: BrowserRuntime,
        retry: FixArguments,
        summary: "A browser asset path could not be normalized or matched to the prepared plan.",
    };
    BrowserDuplicateAsset => {
        code: "E_BROWSER_DUPLICATE_ASSET",
        title: "Duplicate browser asset",
        category: BrowserRuntime,
        retry: FixArguments,
        summary: "The browser supplied more than one asset for the same normalized path.",
    };
    BrowserMissingAsset => {
        code: "E_BROWSER_MISSING_ASSET",
        title: "Missing browser asset",
        category: BrowserRuntime,
        retry: FixEnvironment,
        summary: "The browser did not supply an asset required by the compiled program.",
    };
    BrowserRenderJson => {
        code: "E_BROWSER_RENDER_JSON",
        title: "Invalid browser render plan JSON",
        category: BrowserRuntime,
        retry: FixEnvironment,
        summary: "The browser render plan could not be serialized or decoded as the expected JSON format.",
    };
    BrowserRenderLimit => {
        code: "E_BROWSER_RENDER_LIMIT",
        title: "Browser render limit exceeded",
        category: BrowserRuntime,
        retry: FixSource,
        summary: "The prepared render exceeds a browser runtime work, size, or duration limit.",
    };
    BrowserRenderUnsupported => {
        code: "E_BROWSER_RENDER_UNSUPPORTED",
        title: "Unsupported browser render operation",
        category: BrowserRuntime,
        retry: RetryWillNotHelp,
        summary: "The browser renderer does not support an operation required by this prepared plan.",
    };
    CacheIo => {
        code: "E_CACHE_IO",
        title: "Cache I/O failure",
        category: CacheAndFilesystem,
        retry: RetryMayHelp,
        summary: "ClipAsm could not read, write, create, or inspect a cache path.",
    };
    CacheLock => {
        code: "E_CACHE_LOCK",
        title: "Cache lock unavailable",
        category: CacheAndFilesystem,
        retry: RetryAfterExternalChange,
        summary: "ClipAsm could not acquire the filesystem lock protecting a cache entry.",
    };
    CompiledJson => {
        code: "E_COMPILED_JSON",
        title: "Invalid compiled JSON",
        category: CompilationAndTimelines,
        retry: RetryWillNotHelp,
        summary: "Compiled-program JSON could not be serialized or decoded in the expected format.",
    };
    CrossfadeAudioDuration => {
        code: "E_CROSSFADE_AUDIO_DURATION",
        title: "Invalid crossfade audio duration",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "Crossfade audio inputs cannot provide the exact overlap duration required by the transition.",
    };
    DeclarationAfterStatement => {
        code: "E_DECLARATION_AFTER_STATEMENT",
        title: "Declaration after statement",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A declaration appears after executable statements, where the source grammar no longer permits it.",
    };
    DependencyCycle => {
        code: "E_DEPENDENCY_CYCLE",
        title: "Dependency cycle",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "Program or value dependencies form a cycle that cannot be evaluated in order.",
    };
    DivisionByZero => {
        code: "E_DIVISION_BY_ZERO",
        title: "Division by zero",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A scalar expression attempted exact division by zero.",
    };
    DuplicateArgument => {
        code: "E_DUPLICATE_ARGUMENT",
        title: "Duplicate argument",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A program call supplies the same named argument more than once.",
    };
    DuplicateConfig => {
        code: "E_DUPLICATE_CONFIG",
        title: "Duplicate configuration block",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A source file declares more than one top-level configuration block.",
    };
    DuplicateDeclarationField => {
        code: "E_DUPLICATE_DECLARATION_FIELD",
        title: "Duplicate declaration field",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "A declaration repeats a field that may be specified only once.",
    };
    DuplicateExternal => {
        code: "E_DUPLICATE_EXTERNAL",
        title: "Duplicate external declaration",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "A source file declares the same external program name more than once.",
    };
    DuplicateName => {
        code: "E_DUPLICATE_NAME",
        title: "Duplicate name",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "Two declarations introduce the same name in one namespace.",
    };
    DuplicateProgramImport => {
        code: "E_DUPLICATE_PROGRAM_IMPORT",
        title: "Duplicate program import",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "The same program is imported more than once into a declaration.",
    };
    EmptyConcat => {
        code: "E_EMPTY_CONCAT",
        title: "Empty concat input",
        category: TypesAndStack,
        retry: FixSource,
        summary: "The concat built-in received no values even though at least one is required.",
    };
    EmptyJoin => {
        code: "E_EMPTY_JOIN",
        title: "Empty join result",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A graph join operation has no input values from which to produce an output.",
    };
    EntrypointOutputCount => {
        code: "E_ENTRYPOINT_OUTPUT_COUNT",
        title: "Wrong entrypoint output count",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "The root program produced a different number of outputs than the entrypoint contract permits.",
    };
    ExpectedStatementEnd => {
        code: "E_EXPECTED_STATEMENT_END",
        title: "Expected statement end",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The parser expected a newline or closing delimiter after a complete statement.",
    };
    ExpectedToken => {
        code: "E_EXPECTED_TOKEN",
        title: "Expected token",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The parser did not find the required token at the highlighted source location.",
    };
    ExportDimensions => {
        code: "E_EXPORT_DIMENSIONS",
        title: "Invalid export dimensions",
        category: RenderingAndPublication,
        retry: FixSource,
        summary: "A rendered Video export has dimensions that the selected output contract cannot accept.",
    };
    ExternalChanged => {
        code: "E_EXTERNAL_CHANGED",
        title: "External program changed",
        category: ExternalPrograms,
        retry: RetryAfterExternalChange,
        summary: "An external executable changed after ClipAsm recorded its prepared identity.",
    };
    ExternalExecutable => {
        code: "E_EXTERNAL_EXECUTABLE",
        title: "External executable unavailable",
        category: ExternalPrograms,
        retry: FixEnvironment,
        summary: "ClipAsm could not resolve or inspect the configured external-program executable.",
    };
    ExternalExecution => {
        code: "E_EXTERNAL_EXECUTION",
        title: "External program execution failed",
        category: ExternalPrograms,
        retry: RetryMayHelp,
        summary: "An external program could not be started, completed unsuccessfully, or exceeded execution limits.",
    };
    ExternalProtocol => {
        code: "E_EXTERNAL_PROTOCOL",
        title: "External program protocol failure",
        category: ExternalPrograms,
        retry: FixEnvironment,
        summary: "An external program returned output that does not satisfy ClipAsm's protocol contract.",
    };
    ExternalWithBody => {
        code: "E_EXTERNAL_WITH_BODY",
        title: "External program cannot have a body",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An authored external-program declaration uses a caller body, which external programs do not support.",
    };
    ExternalWithImports => {
        code: "E_EXTERNAL_WITH_IMPORTS",
        title: "External program cannot import programs",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An authored external-program declaration contains imports, which are not supported there.",
    };
    Ffmpeg => {
        code: "E_FFMPEG",
        title: "FFmpeg execution failed",
        category: RenderingAndPublication,
        retry: FixEnvironment,
        summary: "FFmpeg failed while ClipAsm was rendering or encoding an artifact.",
    };
    FfmpegCapability => {
        code: "E_FFMPEG_CAPABILITY",
        title: "Required FFmpeg capability unavailable",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "The selected FFmpeg installation lacks a filter, codec, or feature required by the plan.",
    };
    Ffprobe => {
        code: "E_FFPROBE",
        title: "FFprobe inspection failed",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "FFprobe could not inspect an input or returned unusable media information.",
    };
    Fingerprint => {
        code: "E_FINGERPRINT",
        title: "Fingerprint construction failed",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "ClipAsm could not encode invariant-protected data for a deterministic identity fingerprint.",
    };
    FrameOverflow => {
        code: "E_FRAME_OVERFLOW",
        title: "Frame count overflow",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "An exact duration or timeline calculation exceeded the supported frame-count range.",
    };
    GenericTypeMismatch => {
        code: "E_GENERIC_TYPE_MISMATCH",
        title: "Generic type mismatch",
        category: TypesAndStack,
        retry: FixSource,
        summary: "Arguments to one generic call resolve to different concrete Video and Audio types.",
    };
    GraphTooLarge => {
        code: "E_GRAPH_TOO_LARGE",
        title: "Graph size limit exceeded",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "The authored program exceeded a bounded graph or generated execution-recipe size limit.",
    };
    ImportedOutput => {
        code: "E_IMPORTED_OUTPUT",
        title: "Imported source declares an output",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An imported source declares a root output even though only the entry source may publish one.",
    };
    ImportedProjectSettings => {
        code: "E_IMPORTED_PROJECT_SETTINGS",
        title: "Imported project settings",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An imported source contains project-wide settings that are valid only in the entry source.",
    };
    ImportRequiresFile => {
        code: "E_IMPORT_REQUIRES_FILE",
        title: "Import requires a source file",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An import was requested from source text without a filesystem base for resolving its path.",
    };
    InitConflict => {
        code: "E_INIT_CONFLICT",
        title: "Project initialization conflict",
        category: CommandLineAndProjects,
        retry: FixArguments,
        summary: "Project initialization would overwrite or conflict with an existing path.",
    };
    InitIo => {
        code: "E_INIT_IO",
        title: "Project initialization I/O failure",
        category: CommandLineAndProjects,
        retry: FixEnvironment,
        summary: "ClipAsm could not create, inspect, or write a file needed for project initialization.",
    };
    InitPath => {
        code: "E_INIT_PATH",
        title: "Invalid initialization path",
        category: CommandLineAndProjects,
        retry: FixArguments,
        summary: "The requested project initialization target is not a valid safe destination.",
    };
    InputBodyOutputCount => {
        code: "E_INPUT_BODY_OUTPUT_COUNT",
        title: "Wrong input-body output count",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An input-transforming body produced a different number of values than its built-in requires.",
    };
    InputHash => {
        code: "E_INPUT_HASH",
        title: "Input hashing failed",
        category: CacheAndFilesystem,
        retry: RetryAfterExternalChange,
        summary: "ClipAsm could not read and hash an input needed for deterministic identity.",
    };
    InspectionExists => {
        code: "E_INSPECTION_EXISTS",
        title: "Inspection destination exists",
        category: CommandLineAndProjects,
        retry: FixArguments,
        summary: "An inspection command would overwrite an existing destination without explicit permission.",
    };
    InspectionIo => {
        code: "E_INSPECTION_IO",
        title: "Inspection output I/O failure",
        category: CommandLineAndProjects,
        retry: FixEnvironment,
        summary: "ClipAsm could not create or write the requested inspection output.",
    };
    InternalBinding => {
        code: "E_INTERNAL_BINDING",
        title: "Internal binding failure",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A checked call reached evaluation with a stack binding state the checker should have prevented.",
    };
    InternalExternalProgram => {
        code: "E_INTERNAL_EXTERNAL_PROGRAM",
        title: "Internal external-program failure",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "An external-program definition reached a phase with inconsistent compiler-owned metadata.",
    };
    InternalProgramContract => {
        code: "E_INTERNAL_PROGRAM_CONTRACT",
        title: "Internal program contract failure",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A built-in program implementation disagreed with its compiler-owned signature or body contract.",
    };
    InternalProgramLink => {
        code: "E_INTERNAL_PROGRAM_LINK",
        title: "Internal program link failure",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "The linker could not resolve a program reference that an earlier owning phase should have validated.",
    };
    InternalTypeResolution => {
        code: "E_INTERNAL_TYPE_RESOLUTION",
        title: "Internal type resolution failure",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A checked value reached evaluation without the concrete type guaranteed by type checking.",
    };
    InvalidAccessTarget => {
        code: "E_INVALID_ACCESS_TARGET",
        title: "Invalid stack-access target",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A stack-access modifier was applied to a construct that cannot accept it.",
    };
    InvalidArgumentType => {
        code: "E_INVALID_ARGUMENT_TYPE",
        title: "Invalid argument type",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A call argument has a different scalar or graph value type than its parameter requires.",
    };
    InvalidArgumentValue => {
        code: "E_INVALID_ARGUMENT_VALUE",
        title: "Invalid argument value",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A typed call argument is outside the value domain accepted by its program.",
    };
    InvalidAudioSpec => {
        code: "E_INVALID_AUDIO_SPEC",
        title: "Invalid audio specification",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "An authored Audio declaration contains an invalid rate, channel count, duration, or related fact.",
    };
    InvalidCliBinding => {
        code: "E_INVALID_CLI_BINDING",
        title: "Invalid command-line binding",
        category: CommandLineAndProjects,
        retry: FixArguments,
        summary: "A command-line root binding does not use the name or value shape required by the source.",
    };
    InvalidCrossfadeDuration => {
        code: "E_INVALID_CROSSFADE_DURATION",
        title: "Invalid crossfade duration",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "The requested crossfade duration is zero, negative, or longer than the available transition inputs.",
    };
    InvalidDuration => {
        code: "E_INVALID_DURATION",
        title: "Invalid duration",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A duration literal or computed duration is malformed, negative, or outside the supported range.",
    };
    InvalidEscape => {
        code: "E_INVALID_ESCAPE",
        title: "Invalid string escape",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A string literal contains an escape sequence that ClipAsm does not recognize.",
    };
    InvalidExternalProgram => {
        code: "E_INVALID_EXTERNAL_PROGRAM",
        title: "Invalid external-program declaration",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An authored external-program declaration is incomplete or structurally inconsistent.",
    };
    InvalidFlashCutDuration => {
        code: "E_INVALID_FLASH_CUT_DURATION",
        title: "Invalid flash-cut duration",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "The requested flash-cut duration cannot fit the transition's exact timeline constraints.",
    };
    InvalidGraph => {
        code: "E_INVALID_GRAPH",
        title: "Invalid semantic graph",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A semantic graph violated an invariant that its builder should have enforced.",
    };
    InvalidManifestDestination => {
        code: "E_INVALID_MANIFEST_DESTINATION",
        title: "Invalid manifest destination",
        category: RenderingAndPublication,
        retry: FixArguments,
        summary: "The render-manifest path is unsafe, conflicts with output, or cannot be used as requested.",
    };
    InvalidNumber => {
        code: "E_INVALID_NUMBER",
        title: "Invalid number",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A numeric literal cannot be represented as ClipAsm's exact number type.",
    };
    InvalidOutputBinding => {
        code: "E_INVALID_OUTPUT_BINDING",
        title: "Invalid output binding",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An output binding has a name, position, or shape that does not match the called program.",
    };
    InvalidOutputDestination => {
        code: "E_INVALID_OUTPUT_DESTINATION",
        title: "Invalid output destination",
        category: RenderingAndPublication,
        retry: FixArguments,
        summary: "The requested render output path cannot safely receive the selected artifact.",
    };
    InvalidOutputExtension => {
        code: "E_INVALID_OUTPUT_EXTENSION",
        title: "Invalid output extension",
        category: RenderingAndPublication,
        retry: FixArguments,
        summary: "The output filename extension is missing or incompatible with the rendered media type.",
    };
    InvalidParameterDefault => {
        code: "E_INVALID_PARAMETER_DEFAULT",
        title: "Invalid parameter default",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An authored program parameter default is not a constant value of its declared scalar type.",
    };
    InvalidPlan => {
        code: "E_INVALID_PLAN",
        title: "Invalid prepared plan",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A prepared render plan violated an invariant that preflight should have established.",
    };
    InvalidProgramDefinition => {
        code: "E_INVALID_PROGRAM_DEFINITION",
        title: "Invalid program definition",
        category: InternalContractFailures,
        retry: ReportBug,
        summary: "A registered program definition is internally inconsistent or cannot satisfy registry invariants.",
    };
    InvalidRepeatCount => {
        code: "E_INVALID_REPEAT_COUNT",
        title: "Invalid repeat count",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A repeat count is not a positive supported integer.",
    };
    InvalidScalarOperation => {
        code: "E_INVALID_SCALAR_OPERATION",
        title: "Invalid scalar operation",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A scalar operator was applied to values for which that operation is not defined.",
    };
    InvalidStackAccess => {
        code: "E_INVALID_STACK_ACCESS",
        title: "Invalid stack access",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A call requests stack visibility that its syntax or program contract does not permit.",
    };
    InvalidStatement => {
        code: "E_INVALID_STATEMENT",
        title: "Invalid statement",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A parsed expression or declaration appears where the language requires an executable statement.",
    };
    InvalidTimelineSelector => {
        code: "E_INVALID_TIMELINE_SELECTOR",
        title: "Invalid timeline selector",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A timeline selector is malformed or cannot be applied to the selected value.",
    };
    InvalidTimeRange => {
        code: "E_INVALID_TIME_RANGE",
        title: "Invalid time range",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A time range is reversed, empty where prohibited, or outside the supported exact domain.",
    };
    InvalidToken => {
        code: "E_INVALID_TOKEN",
        title: "Invalid token",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The lexer encountered a character sequence that cannot begin a ClipAsm token.",
    };
    InvalidTypeArgument => {
        code: "E_INVALID_TYPE_ARGUMENT",
        title: "Invalid type argument",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An explicit type argument is not one of the types accepted by the generic program.",
    };
    InvalidVersion => {
        code: "E_INVALID_VERSION",
        title: "Invalid language version",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The source version declaration is malformed or does not contain a valid version number.",
    };
    InvalidVideoSpec => {
        code: "E_INVALID_VIDEO_SPEC",
        title: "Invalid video specification",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "An authored Video declaration contains invalid dimensions, frame rate, duration, or related facts.",
    };
    InvalidZoomAmount => {
        code: "E_INVALID_ZOOM_AMOUNT",
        title: "Invalid zoom amount",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A zoom amount is outside the positive range supported by the zoom effect.",
    };
    Manifest => {
        code: "E_MANIFEST",
        title: "Render manifest failure",
        category: RenderingAndPublication,
        retry: FixEnvironment,
        summary: "ClipAsm could not construct, serialize, or validate the render manifest.",
    };
    ManifestCollision => {
        code: "E_MANIFEST_COLLISION",
        title: "Manifest path collision",
        category: RenderingAndPublication,
        retry: FixArguments,
        summary: "Two publication products resolve to the same manifest or artifact destination.",
    };
    MissingArgument => {
        code: "E_MISSING_ARGUMENT",
        title: "Missing argument",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A program call omits a required named or positional argument.",
    };
    MissingAudioFile => {
        code: "E_MISSING_AUDIO_FILE",
        title: "Missing audio file",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "An Audio source references a file that preflight cannot find or access.",
    };
    MissingExternalField => {
        code: "E_MISSING_EXTERNAL_FIELD",
        title: "Missing external-program field",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An external-program declaration omits a field required to define its executable contract.",
    };
    MissingExternalFile => {
        code: "E_MISSING_EXTERNAL_FILE",
        title: "Missing external-program file",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An external-program declaration does not specify the source or executable file it requires.",
    };
    MissingImageDuration => {
        code: "E_MISSING_IMAGE_DURATION",
        title: "Missing image duration",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An image call needs an explicit duration because no enclosing timeline context can supply one.",
    };
    MissingImageFile => {
        code: "E_MISSING_IMAGE_FILE",
        title: "Missing image file",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "An image source references a file that preflight cannot find or access.",
    };
    MissingImportAlias => {
        code: "E_MISSING_IMPORT_ALIAS",
        title: "Missing import alias",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An import form that requires a local alias does not provide one.",
    };
    MissingKeywordValues => {
        code: "E_MISSING_KEYWORD_VALUES",
        title: "Missing keyword choices",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "An authored Keyword parameter declaration does not list any allowed values.",
    };
    MissingOutput => {
        code: "E_MISSING_OUTPUT",
        title: "Missing output declaration",
        category: RenderingAndPublication,
        retry: FixSource,
        summary: "The entry source does not identify the value or path that should be published.",
    };
    MissingReference => {
        code: "E_MISSING_REFERENCE",
        title: "Missing named reference",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A timeline expression refers to a named output or placement that does not exist.",
    };
    MissingRequiredInput => {
        code: "E_MISSING_REQUIRED_INPUT",
        title: "Missing required stack input",
        category: TypesAndStack,
        retry: FixSource,
        summary: "The current stack does not contain a value required to bind a program input.",
    };
    MissingVersion => {
        code: "E_MISSING_VERSION",
        title: "Missing language version",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A ClipAsm source file does not begin with the required language version declaration.",
    };
    MissingVideoFile => {
        code: "E_MISSING_VIDEO_FILE",
        title: "Missing video file",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "A Video source references a file that preflight cannot find or access.",
    };
    MixedGraphArgumentStyles => {
        code: "E_MIXED_GRAPH_ARGUMENT_STYLES",
        title: "Mixed graph argument styles",
        category: TypesAndStack,
        retry: FixSource,
        summary: "One call mixes explicit graph-valued arguments with implicit stack binding.",
    };
    NumberTooLarge => {
        code: "E_NUMBER_TOO_LARGE",
        title: "Number is too large",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A numeric value exceeds the exact integer range supported by the operation that consumes it.",
    };
    OutputBindingCount => {
        code: "E_OUTPUT_BINDING_COUNT",
        title: "Wrong output binding count",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A call declares a different number of output bindings than the program returns.",
    };
    OutputCollision => {
        code: "E_OUTPUT_COLLISION",
        title: "Output destination collision",
        category: RenderingAndPublication,
        retry: FixArguments,
        summary: "More than one rendered product resolves to the same output destination.",
    };
    OutputIo => {
        code: "E_OUTPUT_IO",
        title: "Output I/O failure",
        category: RenderingAndPublication,
        retry: FixEnvironment,
        summary: "ClipAsm could not create, write, move, or inspect a render output path.",
    };
    ParameterNotValue => {
        code: "E_PARAMETER_NOT_VALUE",
        title: "Parameter is not a value",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A scalar parameter name is used where the language requires a graph or concrete runtime value.",
    };
    PathResolution => {
        code: "E_PATH_RESOLUTION",
        title: "Path resolution failed",
        category: CacheAndFilesystem,
        retry: FixEnvironment,
        summary: "A source-relative or destination path could not be normalized or resolved safely.",
    };
    PositionalAfterNamed => {
        code: "E_POSITIONAL_AFTER_NAMED",
        title: "Positional argument after named argument",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A call supplies a positional argument after named arguments have begun.",
    };
    PreparedJson => {
        code: "E_PREPARED_JSON",
        title: "Invalid prepared JSON",
        category: PreflightAndMedia,
        retry: RetryWillNotHelp,
        summary: "A prepared render plan could not be serialized or decoded as the expected JSON format.",
    };
    ProgramImportCollision => {
        code: "E_PROGRAM_IMPORT_COLLISION",
        title: "Program import collision",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An imported program name collides with another imported or locally declared name.",
    };
    ProgramImportCycle => {
        code: "E_PROGRAM_IMPORT_CYCLE",
        title: "Program import cycle",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "Authored program declarations import one another in a cycle.",
    };
    ProgramImportDepth => {
        code: "E_PROGRAM_IMPORT_DEPTH",
        title: "Program import depth exceeded",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "Nested authored program imports exceed ClipAsm's supported linking depth.",
    };
    ProgramOutputCount => {
        code: "E_PROGRAM_OUTPUT_COUNT",
        title: "Wrong program output count",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An authored program body produces a different number of outputs than its declaration promises.",
    };
    ProgramOutputType => {
        code: "E_PROGRAM_OUTPUT_TYPE",
        title: "Wrong program output type",
        category: TypesAndStack,
        retry: FixSource,
        summary: "An authored program body produces an output whose type differs from its declaration.",
    };
    ProjectIo => {
        code: "E_PROJECT_IO",
        title: "Project manifest I/O failure",
        category: CommandLineAndProjects,
        retry: FixEnvironment,
        summary: "ClipAsm could not discover, inspect, or read a project manifest path.",
    };
    ProjectManifest => {
        code: "E_PROJECT_MANIFEST",
        title: "Invalid project manifest",
        category: CommandLineAndProjects,
        retry: FixProject,
        summary: "A clipasm.toml file is malformed or contains an unsupported project setting.",
    };
    ProjectNotFound => {
        code: "E_PROJECT_NOT_FOUND",
        title: "Project manifest not found",
        category: CommandLineAndProjects,
        retry: FixProject,
        summary: "A command omitted its source path, but no clipasm.toml file was found in the current directory or its parents.",
    };
    Publication => {
        code: "E_PUBLICATION",
        title: "Publication failed",
        category: RenderingAndPublication,
        retry: FixEnvironment,
        summary: "ClipAsm could not atomically publish a staged artifact to its requested destination.",
    };
    PublicationLock => {
        code: "E_PUBLICATION_LOCK",
        title: "Publication lock unavailable",
        category: RenderingAndPublication,
        retry: RetryAfterExternalChange,
        summary: "ClipAsm could not acquire the filesystem lock protecting an output publication.",
    };
    RelativePathWithoutBase => {
        code: "E_RELATIVE_PATH_WITHOUT_BASE",
        title: "Relative path has no base",
        category: CacheAndFilesystem,
        retry: FixArguments,
        summary: "A relative path was supplied through an API that has no source file or base directory.",
    };
    RenderAudioTimeline => {
        code: "E_RENDER_AUDIO_TIMELINE",
        title: "Invalid audio render timeline",
        category: RenderingAndPublication,
        retry: FixSource,
        summary: "An Audio render timeline cannot be represented by the exact sample ranges required for execution.",
    };
    ScalarNotValue => {
        code: "E_SCALAR_NOT_VALUE",
        title: "Scalar is not a graph value",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A scalar expression is used where a Video or Audio graph value is required.",
    };
    SourceContract => {
        code: "E_SOURCE_CONTRACT",
        title: "Source contract failure",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "A probed media source does not satisfy the stream, duration, or decodability contract required by its ClipAsm source kind.",
    };
    SourceDecodability => {
        code: "E_SOURCE_DECODABILITY",
        title: "Source is not decodable",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "Media probing succeeded partially, but the selected source stream cannot be decoded as required.",
    };
    SourceExtension => {
        code: "E_SOURCE_EXTENSION",
        title: "Invalid source extension",
        category: ParsingAndSource,
        retry: FixArguments,
        summary: "A source path does not use the file extension required for a ClipAsm source.",
    };
    SourceIo => {
        code: "E_SOURCE_IO",
        title: "Source I/O failure",
        category: CacheAndFilesystem,
        retry: FixEnvironment,
        summary: "ClipAsm could not read, canonicalize, or inspect a source file.",
    };
    SourceWithoutBase => {
        code: "E_SOURCE_WITHOUT_BASE",
        title: "Source has no filesystem base",
        category: ImportsAndDeclarations,
        retry: FixArguments,
        summary: "In-memory source attempted an operation that requires a source-file directory.",
    };
    StackUnderflow => {
        code: "E_STACK_UNDERFLOW",
        title: "Stack underflow",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A statement or call needs more stack values than are currently available.",
    };
    SyntaxNestingDepth => {
        code: "E_SYNTAX_NESTING_DEPTH",
        title: "Syntax nesting limit exceeded",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "Nested expressions or delimiters exceed the parser's supported syntax depth.",
    };
    TimelinePlacementConflict => {
        code: "E_TIMELINE_PLACEMENT_CONFLICT",
        title: "Timeline placement conflict",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "Two authored placements assign incompatible ranges to the same output timeline.",
    };
    TimelineRootMismatch => {
        code: "E_TIMELINE_ROOT_MISMATCH",
        title: "Timeline root mismatch",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "Values combined by one timeline operation do not share the required timeline root.",
    };
    TimeNotFrameAligned => {
        code: "E_TIME_NOT_FRAME_ALIGNED",
        title: "Time is not frame-aligned",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "An exact Video time does not fall on a frame boundary for the active frame rate.",
    };
    TimeNotSampleAligned => {
        code: "E_TIME_NOT_SAMPLE_ALIGNED",
        title: "Time is not sample-aligned",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "An exact Audio time does not fall on a sample boundary for the active sample rate.",
    };
    ToolChanged => {
        code: "E_TOOL_CHANGED",
        title: "Media tool changed",
        category: PreflightAndMedia,
        retry: RetryAfterExternalChange,
        summary: "FFmpeg or FFprobe changed after ClipAsm recorded the tool identity used for preparation.",
    };
    ToolOutputLimit => {
        code: "E_TOOL_OUTPUT_LIMIT",
        title: "Media tool output limit exceeded",
        category: PreflightAndMedia,
        retry: FixEnvironment,
        summary: "FFmpeg, FFprobe, or an external tool produced more output than ClipAsm safely accepts.",
    };
    TooManyPositionalArguments => {
        code: "E_TOO_MANY_POSITIONAL_ARGUMENTS",
        title: "Too many positional arguments",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A program call supplies more positional arguments than its signature has slots.",
    };
    TypeInferenceDependency => {
        code: "E_TYPE_INFERENCE_DEPENDENCY",
        title: "Type inference dependency failure",
        category: TypesAndStack,
        retry: FixSource,
        summary: "Generic type inference depends on another unresolved value or stack selection and needs explicit Video or Audio context.",
    };
    TypeMismatch => {
        code: "E_TYPE_MISMATCH",
        title: "Value type mismatch",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A Video, Audio, or scalar value is used where a different concrete type is required.",
    };
    UnexpectedProgramBody => {
        code: "E_UNEXPECTED_PROGRAM_BODY",
        title: "Unexpected program body",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A call supplies a body to a program whose contract rejects caller-provided bodies.",
    };
    UnexpectedSugarArgument => {
        code: "E_UNEXPECTED_SUGAR_ARGUMENT",
        title: "Unexpected clip argument",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A `clip` form contains an argument, but `clip` accepts only a body and an optional type argument.",
    };
    UnexpectedTypeArgument => {
        code: "E_UNEXPECTED_TYPE_ARGUMENT",
        title: "Unexpected type argument",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A call supplies an explicit type argument to a program that is not generic.",
    };
    UnknownAudioField => {
        code: "E_UNKNOWN_AUDIO_FIELD",
        title: "Unknown audio field",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "An Audio declaration contains a field name that ClipAsm does not recognize.",
    };
    UnknownBuiltinProgram => {
        code: "E_UNKNOWN_BUILTIN_PROGRAM",
        title: "Unknown built-in program",
        category: CommandLineAndProjects,
        retry: FixArguments,
        summary: "A built-in program lookup requested a name that is not registered with ClipAsm.",
    };
    UnknownConfigField => {
        code: "E_UNKNOWN_CONFIG_FIELD",
        title: "Unknown configuration field",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A configuration block contains a field name that this ClipAsm version does not recognize.",
    };
    UnknownDiagnosticCode => {
        code: "E_UNKNOWN_DIAGNOSTIC_CODE",
        title: "Unknown diagnostic code",
        category: CommandLineAndProjects,
        retry: FixArguments,
        summary: "The explain command received a code that is not in ClipAsm's built-in diagnostic catalog.",
    };
    UnknownExternalField => {
        code: "E_UNKNOWN_EXTERNAL_FIELD",
        title: "Unknown external-program field",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "An external-program declaration contains a field name that ClipAsm does not recognize.",
    };
    UnknownParameterType => {
        code: "E_UNKNOWN_PARAMETER_TYPE",
        title: "Unknown parameter type",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "An authored program parameter declaration names a scalar type the language does not recognize.",
    };
    UnknownProgram => {
        code: "E_UNKNOWN_PROGRAM",
        title: "Unknown program",
        category: ImportsAndDeclarations,
        retry: FixSource,
        summary: "ClipAsm could not resolve a called name as a built-in, imported, or locally declared program.",
    };
    UnknownProgramArgument => {
        code: "E_UNKNOWN_PROGRAM_ARGUMENT",
        title: "Unknown program argument",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A program call uses an argument name that is not present in the program's signature.",
    };
    UnknownTimelinePlacement => {
        code: "E_UNKNOWN_TIMELINE_PLACEMENT",
        title: "Unknown timeline placement",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "A timeline selector does not match any authored placement in the selected value.",
    };
    UnknownValueType => {
        code: "E_UNKNOWN_VALUE_TYPE",
        title: "Unknown value type",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "An authored input or output declaration names a graph value type other than Video or Audio.",
    };
    UnknownVideoField => {
        code: "E_UNKNOWN_VIDEO_FIELD",
        title: "Unknown video field",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "A Video declaration contains a field name that ClipAsm does not recognize.",
    };
    UnresolvedLocalType => {
        code: "E_UNRESOLVED_LOCAL_TYPE",
        title: "Unresolved local type",
        category: TypesAndStack,
        retry: FixSource,
        summary: "A local output or reference has insufficient context for ClipAsm to infer its concrete type.",
    };
    UnresolvedTimeline => {
        code: "E_UNRESOLVED_TIMELINE",
        title: "Unresolved timeline",
        category: CompilationAndTimelines,
        retry: FixSource,
        summary: "Compilation could not derive the exact timeline layout required by a later operation.",
    };
    UnsupportedVersion => {
        code: "E_UNSUPPORTED_VERSION",
        title: "Unsupported language version",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The source requests a ClipAsm language version this binary does not support.",
    };
    UnterminatedBlock => {
        code: "E_UNTERMINATED_BLOCK",
        title: "Unterminated block",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The source ended before a program, stack, or clip body received its closing delimiter.",
    };
    UnterminatedConfig => {
        code: "E_UNTERMINATED_CONFIG",
        title: "Unterminated configuration block",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The source ended before the configuration block received its closing delimiter.",
    };
    UnterminatedExternal => {
        code: "E_UNTERMINATED_EXTERNAL",
        title: "Unterminated external declaration",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The source ended before an external-program declaration received its closing delimiter.",
    };
    UnterminatedString => {
        code: "E_UNTERMINATED_STRING",
        title: "Unterminated string",
        category: ParsingAndSource,
        retry: FixSource,
        summary: "The source ended before a string literal received its closing quote.",
    };
}

static AMBIGUOUS_GENERIC_TYPE_CAUSES: [&str; 2] = [
    "A generic call has no typed graph input from which to infer Video or Audio.",
    "Inputs or expected outputs provide conflicting concrete types.",
];
static AMBIGUOUS_GENERIC_TYPE_ACTIONS: [&str; 2] = [
    "Supply a typed input or place the result where one concrete type is required.",
    "Check that every graph argument to the call uses the same concrete type.",
];
static CACHE_LOCK_CAUSES: [&str; 2] = [
    "Another ClipAsm process currently owns the cache-entry lock.",
    "A previous process ended while the filesystem still considers its lock active.",
];
static CACHE_LOCK_ACTIONS: [&str; 2] = [
    "Wait for the other ClipAsm process to finish, then retry.",
    "Confirm no process is using the cache before investigating a persistent lock.",
];
static EXPECTED_TOKEN_CAUSES: [&str; 2] = [
    "A delimiter, argument separator, name, or required keyword is missing.",
    "An earlier malformed construct caused the parser to stop at this token.",
];
static EXPECTED_TOKEN_ACTIONS: [&str; 2] = [
    "Inspect the highlighted token and the construct immediately before it.",
    "Compare that construct with the corresponding language-reference example.",
];
static EXTERNAL_PROTOCOL_CAUSES: [&str; 2] = [
    "The external program wrote malformed, incomplete, or unexpected protocol output.",
    "The executable implements a different ClipAsm external-protocol contract.",
];
static EXTERNAL_PROTOCOL_ACTIONS: [&str; 2] = [
    "Capture the external program's non-sensitive output and validate its response shape.",
    "Verify that the configured executable implements the protocol expected by this ClipAsm version.",
];
static FFMPEG_CAPABILITY_CAUSES: [&str; 2] = [
    "The installed FFmpeg build omits a required filter, encoder, decoder, or format.",
    "A different FFmpeg executable was selected from the environment than expected.",
];
static FFMPEG_CAPABILITY_ACTIONS: [&str; 2] = [
    "Inspect the missing capability named in the original diagnostic.",
    "Install or select an FFmpeg build that provides that capability.",
];
static INTERNAL_CONTRACT_CAUSES: [&str; 2] = [
    "ClipAsm reached an invariant-protected state that an earlier phase should have rejected.",
    "Valid-looking user input may have exposed an implementation defect.",
];
static INTERNAL_CONTRACT_ACTIONS: [&str; 2] = [
    "Preserve the code, ClipAsm version, reproduction steps, and only non-sensitive inputs.",
    "Report the defect without deleting caches or generated state unless maintainers request it.",
];
static INVALID_ARGUMENT_TYPE_CAUSES: [&str; 2] = [
    "A named or positional argument has a different declared type than its parameter.",
    "Implicit stack binding selected a Video or Audio value of the wrong type.",
];
static INVALID_ARGUMENT_TYPE_ACTIONS: [&str; 2] = [
    "Compare the argument named by the diagnostic with the program's call shape.",
    "Correct the explicit argument or the values available for stack binding.",
];
static MISSING_ARGUMENT_CAUSES: [&str; 2] = [
    "A required scalar or explicitly bound graph argument was omitted.",
    "The call uses a different program signature than the author expected.",
];
static MISSING_ARGUMENT_ACTIONS: [&str; 2] = [
    "Add the argument named by the original diagnostic.",
    "Run `clipasm programs NAME` when the call targets a built-in program.",
];
static MISSING_ASSET_CAUSES: [&str; 2] = [
    "The authored path does not exist relative to the source file that supplied it.",
    "The file is inaccessible or changed location before preflight.",
];
static MISSING_ASSET_ACTIONS: [&str; 2] = [
    "Resolve the displayed path from its supplying source file and verify the file exists.",
    "Correct the authored path or restore readable access to the asset.",
];
static PUBLICATION_CAUSES: [&str; 2] = [
    "The destination changed, became inaccessible, or rejected the atomic replacement.",
    "A filesystem boundary or policy prevents moving the staged artifact into place.",
];
static PUBLICATION_ACTIONS: [&str; 2] = [
    "Check the destination's permissions, free space, and current file type.",
    "Keep the staged artifact and original diagnostic available while resolving the destination.",
];
static TIMELINE_CONFLICT_CAUSES: [&str; 2] = [
    "Two named or inherited placements assign incompatible source ranges to one result.",
    "A replacement or selection overlaps a placement that must remain unique.",
];
static TIMELINE_CONFLICT_ACTIONS: [&str; 2] = [
    "Inspect the conflicting placements named in the original notes.",
    "Adjust their selectors or ranges so the result has one unambiguous layout.",
];
static UNKNOWN_BUILTIN_CAUSES: [&str; 2] = [
    "The requested built-in name is misspelled.",
    "The name belongs to an imported or authored program, not ClipAsm's built-in catalog.",
];
static UNKNOWN_BUILTIN_ACTIONS: [&str; 2] = [
    "Run `clipasm programs` and use an exact listed built-in name.",
    "Use project compilation, rather than built-in lookup, for imported or authored programs.",
];
static UNKNOWN_DIAGNOSTIC_CAUSES: [&str; 2] = [
    "The code was mistyped, truncated, or copied with extra characters.",
    "The code belongs to an embedding application or a different ClipAsm version.",
];
static UNKNOWN_DIAGNOSTIC_ACTIONS: [&str; 2] = [
    "Copy the complete `E_...` code from the original diagnostic and check its spelling.",
    "Search the diagnostic index when the code came from another ClipAsm version.",
];
static UNKNOWN_PROGRAM_CAUSES: [&str; 3] = [
    "The program name is misspelled.",
    "The source defining the program was not imported.",
    "An import alias or locally declared name differs from the call.",
];
static UNKNOWN_PROGRAM_ACTIONS: [&str; 3] = [
    "Run `clipasm programs` when the intended target is a built-in program.",
    "Check imports, aliases, and the exact program spelling.",
    "Use the source location and notes from the original diagnostic.",
];
static BROWSER_MISSING_ASSET_CAUSES: [&str; 2] = [
    "The host did not provide bytes for a path required by the browser plan.",
    "The supplied browser path was normalized differently from the compiled asset path.",
];
static BROWSER_MISSING_ASSET_ACTIONS: [&str; 2] = [
    "Provide the exact asset path listed by the original diagnostic.",
    "Recompile and prepare after changing the browser's virtual-file bindings.",
];

const fn specific_common_causes(diagnostic: BuiltinDiagnostic) -> Option<&'static [&'static str]> {
    match diagnostic {
        BuiltinDiagnostic::AmbiguousGenericType => Some(&AMBIGUOUS_GENERIC_TYPE_CAUSES),
        BuiltinDiagnostic::CacheLock => Some(&CACHE_LOCK_CAUSES),
        BuiltinDiagnostic::ExpectedToken => Some(&EXPECTED_TOKEN_CAUSES),
        BuiltinDiagnostic::ExternalProtocol => Some(&EXTERNAL_PROTOCOL_CAUSES),
        BuiltinDiagnostic::FfmpegCapability => Some(&FFMPEG_CAPABILITY_CAUSES),
        BuiltinDiagnostic::InternalBinding
        | BuiltinDiagnostic::InternalExternalProgram
        | BuiltinDiagnostic::InternalProgramContract
        | BuiltinDiagnostic::InternalProgramLink
        | BuiltinDiagnostic::InternalTypeResolution
        | BuiltinDiagnostic::ArtifactContract => Some(&INTERNAL_CONTRACT_CAUSES),
        BuiltinDiagnostic::InvalidArgumentType => Some(&INVALID_ARGUMENT_TYPE_CAUSES),
        BuiltinDiagnostic::MissingArgument => Some(&MISSING_ARGUMENT_CAUSES),
        BuiltinDiagnostic::MissingAudioFile
        | BuiltinDiagnostic::MissingImageFile
        | BuiltinDiagnostic::MissingVideoFile => Some(&MISSING_ASSET_CAUSES),
        BuiltinDiagnostic::Publication => Some(&PUBLICATION_CAUSES),
        BuiltinDiagnostic::TimelinePlacementConflict => Some(&TIMELINE_CONFLICT_CAUSES),
        BuiltinDiagnostic::UnknownBuiltinProgram => Some(&UNKNOWN_BUILTIN_CAUSES),
        BuiltinDiagnostic::UnknownDiagnosticCode => Some(&UNKNOWN_DIAGNOSTIC_CAUSES),
        BuiltinDiagnostic::UnknownProgram => Some(&UNKNOWN_PROGRAM_CAUSES),
        BuiltinDiagnostic::BrowserMissingAsset => Some(&BROWSER_MISSING_ASSET_CAUSES),
        _ => None,
    }
}

const fn specific_recommended_actions(
    diagnostic: BuiltinDiagnostic,
) -> Option<&'static [&'static str]> {
    match diagnostic {
        BuiltinDiagnostic::AmbiguousGenericType => Some(&AMBIGUOUS_GENERIC_TYPE_ACTIONS),
        BuiltinDiagnostic::CacheLock => Some(&CACHE_LOCK_ACTIONS),
        BuiltinDiagnostic::ExpectedToken => Some(&EXPECTED_TOKEN_ACTIONS),
        BuiltinDiagnostic::ExternalProtocol => Some(&EXTERNAL_PROTOCOL_ACTIONS),
        BuiltinDiagnostic::FfmpegCapability => Some(&FFMPEG_CAPABILITY_ACTIONS),
        BuiltinDiagnostic::InternalBinding
        | BuiltinDiagnostic::InternalExternalProgram
        | BuiltinDiagnostic::InternalProgramContract
        | BuiltinDiagnostic::InternalProgramLink
        | BuiltinDiagnostic::InternalTypeResolution
        | BuiltinDiagnostic::ArtifactContract => Some(&INTERNAL_CONTRACT_ACTIONS),
        BuiltinDiagnostic::InvalidArgumentType => Some(&INVALID_ARGUMENT_TYPE_ACTIONS),
        BuiltinDiagnostic::MissingArgument => Some(&MISSING_ARGUMENT_ACTIONS),
        BuiltinDiagnostic::MissingAudioFile
        | BuiltinDiagnostic::MissingImageFile
        | BuiltinDiagnostic::MissingVideoFile => Some(&MISSING_ASSET_ACTIONS),
        BuiltinDiagnostic::Publication => Some(&PUBLICATION_ACTIONS),
        BuiltinDiagnostic::TimelinePlacementConflict => Some(&TIMELINE_CONFLICT_ACTIONS),
        BuiltinDiagnostic::UnknownBuiltinProgram => Some(&UNKNOWN_BUILTIN_ACTIONS),
        BuiltinDiagnostic::UnknownDiagnosticCode => Some(&UNKNOWN_DIAGNOSTIC_ACTIONS),
        BuiltinDiagnostic::UnknownProgram => Some(&UNKNOWN_PROGRAM_ACTIONS),
        BuiltinDiagnostic::BrowserMissingAsset => Some(&BROWSER_MISSING_ASSET_ACTIONS),
        _ => None,
    }
}

/// Return every catalog reference in code order.
pub(crate) const fn references() -> &'static [DiagnosticReference] {
    REFERENCES
}

/// Return the reference for one exact built-in code.
pub(crate) fn reference(code: &str) -> Option<&'static DiagnosticReference> {
    REFERENCES
        .binary_search_by_key(&code, |diagnostic| diagnostic.code)
        .ok()
        .map(|index| &REFERENCES[index])
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{BuiltinDiagnostic, DiagnosticCategory, REFERENCES, RetryGuidance, reference};

    #[test]
    fn catalog_is_complete_unique_valid_and_ordered() {
        let mut previous = None;
        let mut codes = HashSet::new();
        for diagnostic in REFERENCES {
            assert!(
                diagnostic.code.starts_with("E_")
                    && diagnostic
                        .code
                        .bytes()
                        .skip(2)
                        .all(|byte| byte.is_ascii_uppercase()
                            || byte.is_ascii_digit()
                            || byte == b'_')
            );
            assert!(diagnostic.code.len() > 2);
            assert!(codes.insert(diagnostic.code));
            if let Some(previous) = previous {
                assert!(previous < diagnostic.code);
            }
            previous = Some(diagnostic.code);

            assert!(!diagnostic.title.trim().is_empty());
            assert!(!diagnostic.summary.trim().is_empty());
            assert!(!diagnostic.common_causes().is_empty());
            assert!(!diagnostic.recommended_actions().is_empty());
            assert!(
                std::path::Path::new(diagnostic.category_documentation_route())
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("html"))
            );
            assert!(
                diagnostic
                    .documentation_route()
                    .ends_with(&diagnostic.documentation_anchor())
            );
            assert_eq!(
                diagnostic.documentation_anchor(),
                diagnostic.code.to_ascii_lowercase()
            );
            assert_eq!(diagnostic.diagnostic.code(), diagnostic.code);
        }
    }

    #[test]
    fn lookup_uses_exact_codes() {
        assert_eq!(
            reference("E_UNKNOWN_PROGRAM").map(|entry| entry.diagnostic()),
            Some(BuiltinDiagnostic::UnknownProgram)
        );
        assert!(reference("e_unknown_program").is_none());
        assert!(reference("not-a-built-in-code").is_none());
    }

    #[test]
    fn every_category_is_represented() {
        for category in DiagnosticCategory::ALL {
            assert!(
                REFERENCES
                    .iter()
                    .any(|diagnostic| diagnostic.category == category)
            );
        }
    }

    #[test]
    fn internal_diagnostics_are_reportable() {
        for diagnostic in REFERENCES {
            if diagnostic.category == DiagnosticCategory::InternalContractFailures {
                assert_eq!(diagnostic.retry_guidance, RetryGuidance::ReportBug);
            }
        }
    }
}
