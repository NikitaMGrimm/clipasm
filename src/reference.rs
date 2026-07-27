//! Read-only reference information for `ClipAsm`'s built-in programs.
//!
//! The compiler and this module consume the same internal built-in catalog.
//! This view intentionally omits implementation functions, internal program
//! identifiers, and semantic versions. Changing summaries or examples cannot
//! change compiled or prepared meaning.

use std::fmt;

use crate::model::Number;
use crate::program::{
    BodyOutputConstraint, BuiltinBodyInitialValue, BuiltinDefault,
    BuiltinProgram as CatalogProgram, Cardinality as ProgramCardinality, ParameterType,
    ProgramImplementation, StackAccess as ProgramStackAccess,
    TimelineBehavior as ProgramTimelineBehavior, ValueTypeSpec,
};

mod contracts;
mod diagnostics;

pub use contracts::{
    MachineContract, MachineContractAudience, MachineContractReference, MachineContractStability,
    MachineContractVersion, machine_contract, machine_contracts,
};

pub use diagnostics::{
    DiagnosticCategory, DiagnosticReference, RelatedReference, RetryGuidance, diagnostic,
    diagnostics,
};

/// Return a deterministic snapshot of every built-in program reference.
#[must_use]
pub fn builtin_programs() -> Vec<BuiltinProgram> {
    crate::program::builtin_references()
}

/// Return the reference for one exact built-in program name.
#[must_use]
pub fn builtin_program(name: &str) -> Option<BuiltinProgram> {
    builtin_programs()
        .into_iter()
        .find(|program| program.name == name)
}

/// A useful discovery group for built-in programs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum BuiltinCategory {
    /// File-backed Video and Audio sources.
    Sources,
    /// Timeline composition and range operations.
    Timeline,
    /// Explicit Audio adaptation programs.
    Audio,
    /// Video effects.
    Effects,
    /// Video transitions.
    Transitions,
    /// Programs that evaluate a caller-supplied body.
    BodyPrograms,
}

impl BuiltinCategory {
    /// Categories in their stable display order.
    pub const ALL: [Self; 6] = [
        Self::Sources,
        Self::Timeline,
        Self::Audio,
        Self::Effects,
        Self::Transitions,
        Self::BodyPrograms,
    ];

    /// Return the heading used in human-facing indexes.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sources => "Sources",
            Self::Timeline => "Timeline",
            Self::Audio => "Audio",
            Self::Effects => "Effects",
            Self::Transitions => "Transitions",
            Self::BodyPrograms => "Body programs",
        }
    }
}

/// A sanitized, read-only built-in program reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinProgram {
    name: String,
    category: BuiltinCategory,
    summary: String,
    inputs: Vec<Input>,
    parameters: Vec<Parameter>,
    outputs: Vec<ValueType>,
    stack_access: StackAccess,
    body: Option<BodyContract>,
    timeline_behavior: TimelineBehavior,
    example: String,
    example_expectation: ExampleExpectation,
    diagnostics: Vec<String>,
    behavior_notes: Vec<String>,
    constraints: Vec<String>,
    related_programs: Vec<String>,
}

impl BuiltinProgram {
    #[expect(
        clippy::too_many_lines,
        reason = "the projection explicitly sanitizes every compiler-owned and reference-owned field"
    )]
    pub(crate) fn from_catalog(program: &CatalogProgram) -> Self {
        let descriptor = &program.definition.descriptor;
        let defaults = program.metadata.defaults;
        let inputs = descriptor
            .inputs
            .iter()
            .map(|input| Input {
                name: input.name.clone(),
                value_type: input.value_type.into(),
                cardinality: input.cardinality.into(),
            })
            .collect();
        let parameters = descriptor
            .parameters
            .iter()
            .map(|parameter| Parameter {
                name: parameter.name.clone(),
                kind: (&parameter.parameter_type).into(),
                required: parameter.required,
                default: defaults
                    .iter()
                    .find(|default| default.parameter == parameter.name)
                    .map(|default| default.value.into()),
                omission_behavior: program
                    .metadata
                    .parameter_omissions
                    .iter()
                    .find(|omission| omission.parameter == parameter.name)
                    .map(|omission| omission.behavior.to_owned()),
            })
            .collect();
        let outputs = descriptor.outputs.iter().copied().map(Into::into).collect();
        let body = match &program.definition.implementation {
            ProgramImplementation::Body { contract, .. } => Some(BodyContract {
                initial_values: program
                    .metadata
                    .body_initial_values
                    .iter()
                    .copied()
                    .zip(contract.initial_values.iter().copied())
                    .map(|(role, value_type)| BodyInitialValue {
                        value_type: value_type.into(),
                        role: role.into(),
                    })
                    .collect(),
                outputs: match &contract.outputs {
                    BodyOutputConstraint::Exactly(outputs) => {
                        BodyOutputs::Exactly(outputs.iter().copied().map(Into::into).collect())
                    }
                    BodyOutputConstraint::Variadic { value_type, min } => BodyOutputs::Variadic {
                        value_type: (*value_type).into(),
                        minimum: *min,
                    },
                },
            }),
            ProgramImplementation::Direct(_)
            | ProgramImplementation::ClipAsm(_)
            | ProgramImplementation::External(_) => None,
        };

        Self {
            name: descriptor.name.clone(),
            category: program.metadata.category,
            summary: program.metadata.summary.to_owned(),
            inputs,
            parameters,
            outputs,
            stack_access: descriptor.default_stack_access.into(),
            body,
            timeline_behavior: timeline_behavior(program),
            example: program.metadata.example.to_owned(),
            example_expectation: ExampleExpectation {
                outputs: program
                    .metadata
                    .example_expected_outputs
                    .expect("validated built-in example output expectation")
                    .to_vec(),
                expected_frames: program.metadata.example_expected_frames,
            },
            diagnostics: program
                .metadata
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code().to_owned())
                .collect(),
            behavior_notes: program
                .metadata
                .behavior_notes
                .iter()
                .map(|note| (*note).to_owned())
                .collect(),
            constraints: program
                .metadata
                .constraints
                .iter()
                .map(|constraint| (*constraint).to_owned())
                .collect(),
            related_programs: program
                .metadata
                .related_programs
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }

    /// Return the exact registered name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the discovery category.
    #[must_use]
    pub const fn category(&self) -> BuiltinCategory {
        self.category
    }

    /// Return the concise human-facing summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Return the ordered graph-valued inputs.
    #[must_use]
    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    /// Return the ordered scalar parameters.
    #[must_use]
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// Return the ordered output types.
    #[must_use]
    pub fn outputs(&self) -> &[ValueType] {
        &self.outputs
    }

    /// Return whether the call uses one homogeneous Video-or-Audio type.
    #[must_use]
    pub fn is_generic(&self) -> bool {
        self.inputs
            .iter()
            .any(|input| input.value_type == ValueType::Generic)
            || self.outputs.contains(&ValueType::Generic)
    }

    /// Return the default stack-access policy.
    #[must_use]
    pub const fn stack_access(&self) -> StackAccess {
        self.stack_access
    }

    /// Return the caller-body contract, or `None` when a body is rejected.
    #[must_use]
    pub const fn body(&self) -> Option<&BodyContract> {
        self.body.as_ref()
    }

    /// Return the authored timeline-layout behavior.
    #[must_use]
    pub const fn timeline_behavior(&self) -> &TimelineBehavior {
        &self.timeline_behavior
    }

    /// Return one concise valid `ClipAsm` example.
    #[must_use]
    pub fn example(&self) -> &str {
        &self.example
    }

    /// Return the catalog-owned result expected when validating the example.
    #[must_use]
    pub const fn example_expectation(&self) -> &ExampleExpectation {
        &self.example_expectation
    }

    /// Return selected actionable diagnostic codes.
    ///
    /// This is not a complete list of every diagnostic a call may produce.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Return concise catalog-owned behavior facts.
    #[must_use]
    pub fn behavior_notes(&self) -> &[String] {
        &self.behavior_notes
    }

    /// Return important call constraints.
    #[must_use]
    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    /// Return related built-in program names.
    #[must_use]
    pub fn related_programs(&self) -> &[String] {
        &self.related_programs
    }

    /// Return the stable generated-book route.
    #[must_use]
    pub fn documentation_route(&self) -> String {
        format!("reference/programs/{}.html", self.name)
    }

    /// Return the full hosted-guide URL.
    #[must_use]
    pub fn documentation_url(&self) -> String {
        format!(
            "https://nikitamgrimm.github.io/clipasm/{}",
            self.documentation_route()
        )
    }

    /// Render a labelled pseudo-signature for lookup.
    ///
    /// The returned shape describes a call; it is not declaration syntax.
    #[must_use]
    pub fn call_shape(&self) -> String {
        let mut shape = self.name.clone();
        if self.is_generic() {
            shape.push_str("<T: Video | Audio>");
        }
        shape.push('(');
        let arguments = self
            .inputs
            .iter()
            .map(Input::call_shape)
            .chain(self.parameters.iter().map(Parameter::call_shape))
            .collect::<Vec<_>>();
        shape.push_str(&arguments.join(", "));
        shape.push(')');
        if self.body.is_some() {
            shape.push_str(" { ... }");
        }
        shape.push_str(" -> ");
        if self.outputs.is_empty() {
            shape.push_str("none");
        } else {
            shape.push_str(
                &self
                    .outputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        shape
    }
}

/// Expected pure-compilation result for a built-in example.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleExpectation {
    outputs: Vec<crate::model::ValueType>,
    expected_frames: Option<u64>,
}

impl ExampleExpectation {
    /// Return the exact ordered concrete output types.
    #[must_use]
    pub fn outputs(&self) -> &[crate::model::ValueType] {
        &self.outputs
    }

    /// Return the deterministic single-Video frame count, when compilation can know it.
    #[must_use]
    pub const fn expected_frames(&self) -> Option<u64> {
        self.expected_frames
    }
}

/// One graph-valued program input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Input {
    name: String,
    value_type: ValueType,
    cardinality: Cardinality,
}

impl Input {
    /// Return the input name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the input type.
    #[must_use]
    pub const fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// Return the input cardinality.
    #[must_use]
    pub const fn cardinality(&self) -> Cardinality {
        self.cardinality
    }

    fn call_shape(&self) -> String {
        match self.cardinality {
            Cardinality::One => format!("{}: {}", self.name, self.value_type),
            Cardinality::Variadic { .. } => format!("{}: {}...", self.name, self.value_type),
        }
    }
}

/// The number of graph values accepted by an input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Cardinality {
    /// Exactly one value.
    One,
    /// A variable number of values with an inclusive minimum.
    Variadic {
        /// The fewest accepted values.
        minimum: usize,
    },
}

/// A graph value type in an unresolved built-in signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueType {
    /// A concrete Video.
    Video,
    /// Concrete standalone Audio.
    Audio,
    /// The invocation's single homogeneous Video-or-Audio type.
    Generic,
}

impl fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Generic => "T",
        })
    }
}

/// One scalar program parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: String,
    kind: ParameterTypeReference,
    required: bool,
    default: Option<DefaultValue>,
    omission_behavior: Option<String>,
}

impl Parameter {
    /// Return the parameter name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the scalar type.
    #[must_use]
    pub const fn parameter_type(&self) -> &ParameterTypeReference {
        &self.kind
    }

    /// Return whether the caller must always supply this parameter.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.required
    }

    /// Return the catalog-owned typed default.
    ///
    /// An optional parameter may have no fixed default when its value is
    /// supplied contextually, as with `image.duration`.
    #[must_use]
    pub const fn default(&self) -> Option<&DefaultValue> {
        self.default.as_ref()
    }

    /// Return what happens when an optional parameter has no fixed default.
    ///
    /// Returns `None` for required parameters and parameters with a catalog
    /// default.
    #[must_use]
    pub fn omission_behavior(&self) -> Option<&str> {
        self.omission_behavior.as_deref()
    }

    fn call_shape(&self) -> String {
        let optional = if self.required { "" } else { "?" };
        format!("{}{optional}: {}", self.name, self.kind)
    }
}

/// A scalar parameter type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParameterTypeReference {
    /// An exact rational number.
    Number,
    /// An exact integral number.
    Integer,
    /// A path resolved from its supplying source.
    File,
    /// An exact nonnegative duration.
    Duration,
    /// A concrete or timeline-marker range.
    TimeRange,
    /// One value from a closed keyword set.
    Keyword {
        /// Accepted keyword spellings in declaration order.
        values: Vec<String>,
    },
}

impl fmt::Display for ParameterTypeReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number => formatter.write_str("Number"),
            Self::Integer => formatter.write_str("Integer"),
            Self::File => formatter.write_str("File"),
            Self::Duration => formatter.write_str("Duration"),
            Self::TimeRange => formatter.write_str("TimeRange"),
            Self::Keyword { values } => write!(formatter, "Keyword({})", values.join(" | ")),
        }
    }
}

/// A typed built-in parameter default.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DefaultValue {
    /// An exact Number and its mechanically derived concise spelling.
    Number {
        /// Exact runtime value.
        value: Number,
        /// Concise authored spelling.
        display: String,
    },
    /// A Duration represented exactly in milliseconds.
    DurationMilliseconds(u64),
    /// One accepted keyword.
    Keyword(String),
}

impl fmt::Display for DefaultValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number { display, .. } => formatter.write_str(display),
            Self::DurationMilliseconds(milliseconds) => write!(formatter, "{milliseconds}ms"),
            Self::Keyword(value) => formatter.write_str(value),
        }
    }
}

/// The default stack visibility of a built-in invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StackAccess {
    /// Bind only values owned by the current stack frame.
    Owned,
    /// Bind values visible through the current frame.
    Visible,
}

impl fmt::Display for StackAccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Owned => "owned",
            Self::Visible => "visible",
        })
    }
}

/// The initial stack and required result of a caller-supplied body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyContract {
    initial_values: Vec<BodyInitialValue>,
    outputs: BodyOutputs,
}

impl BodyContract {
    /// Return the values placed on the body stack before evaluation.
    #[must_use]
    pub fn initial_values(&self) -> &[BodyInitialValue] {
        &self.initial_values
    }

    /// Return the required values remaining after body evaluation.
    #[must_use]
    pub const fn outputs(&self) -> &BodyOutputs {
        &self.outputs
    }
}

/// One catalog-owned body initialization rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyInitialValue {
    value_type: ValueType,
    role: BodyInitialValueRole,
}

impl BodyInitialValue {
    /// Return the initial value type.
    #[must_use]
    pub const fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// Return where the initial value comes from.
    #[must_use]
    pub const fn role(&self) -> &BodyInitialValueRole {
        &self.role
    }
}

/// The source of one initial body-stack value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BodyInitialValueRole {
    /// The complete bound input value.
    Input {
        /// Input port name.
        input: String,
    },
    /// The selected range of a bound timeline input.
    SelectedRange {
        /// Timeline input port name.
        input: String,
        /// `TimeRange` parameter selecting the initial value.
        parameter: String,
    },
}

/// The ordered body values required after evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BodyOutputs {
    /// One exact ordered type sequence.
    Exactly(Vec<ValueType>),
    /// A homogeneous variable-length result.
    Variadic {
        /// Required homogeneous type.
        value_type: ValueType,
        /// Inclusive minimum value count.
        minimum: usize,
    },
}

/// How a built-in maps authored timeline placements to its output.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TimelineBehavior {
    /// Create a fresh layout, or no layout when there is no output.
    Fresh,
    /// Preserve one input's layout.
    Identity {
        /// Preserved input name.
        input: String,
    },
    /// Repeat one input's layout according to repeat semantics.
    Repeat {
        /// Repeated input name.
        input: String,
    },
    /// Concatenate a variadic input's layouts.
    Concat {
        /// Concatenated input name.
        input: String,
    },
    /// Concatenate body results initialized from these inputs.
    BodyConcat {
        /// Initial input names in body order.
        inputs: Vec<String>,
    },
    /// Crop one input's layout to a range.
    Crop {
        /// Cropped input name.
        input: String,
    },
    /// Replace a selected range in one base input.
    Replace {
        /// Base input name.
        base: String,
    },
    /// Create sequential before and after transition regions.
    FlashCut {
        /// First Video input.
        before: String,
        /// Second Video input.
        after: String,
    },
    /// Create before, overlap, and after transition regions.
    Crossfade {
        /// First Video input.
        before: String,
        /// Second Video input.
        after: String,
    },
}

impl From<ValueTypeSpec> for ValueType {
    fn from(value_type: ValueTypeSpec) -> Self {
        match value_type {
            ValueTypeSpec::Exact(crate::model::ValueType::Video) => Self::Video,
            ValueTypeSpec::Exact(crate::model::ValueType::Audio) => Self::Audio,
            ValueTypeSpec::Generic => Self::Generic,
        }
    }
}

impl From<ProgramCardinality> for Cardinality {
    fn from(cardinality: ProgramCardinality) -> Self {
        match cardinality {
            ProgramCardinality::One => Self::One,
            ProgramCardinality::Variadic { min } => Self::Variadic { minimum: min },
        }
    }
}

impl From<&ParameterType> for ParameterTypeReference {
    fn from(parameter_type: &ParameterType) -> Self {
        match parameter_type {
            ParameterType::Number => Self::Number,
            ParameterType::Integer => Self::Integer,
            ParameterType::File => Self::File,
            ParameterType::Duration => Self::Duration,
            ParameterType::TimeRange => Self::TimeRange,
            ParameterType::Keyword(values) => Self::Keyword {
                values: values.clone(),
            },
        }
    }
}

impl From<BuiltinDefault> for DefaultValue {
    fn from(default: BuiltinDefault) -> Self {
        match default {
            BuiltinDefault::NumberRatio {
                numerator,
                denominator,
            } => {
                let value = Number::from_ratio(numerator, denominator);
                let percentage_numerator = i128::from(numerator) * 100;
                let display = if percentage_numerator % i128::from(denominator) == 0 {
                    format!("{}%", percentage_numerator / i128::from(denominator))
                } else {
                    value.authored_display()
                };
                Self::Number { value, display }
            }
            BuiltinDefault::DurationMilliseconds(milliseconds) => {
                Self::DurationMilliseconds(milliseconds)
            }
            BuiltinDefault::Keyword(value) => Self::Keyword(value.to_owned()),
        }
    }
}

impl From<ProgramStackAccess> for StackAccess {
    fn from(access: ProgramStackAccess) -> Self {
        match access {
            ProgramStackAccess::Owned => Self::Owned,
            ProgramStackAccess::Visible => Self::Visible,
        }
    }
}

impl From<BuiltinBodyInitialValue> for BodyInitialValueRole {
    fn from(value: BuiltinBodyInitialValue) -> Self {
        match value {
            BuiltinBodyInitialValue::Input(input) => Self::Input {
                input: input.to_owned(),
            },
            BuiltinBodyInitialValue::SelectedRange { input, parameter } => Self::SelectedRange {
                input: input.to_owned(),
                parameter: parameter.to_owned(),
            },
        }
    }
}

fn timeline_behavior(program: &CatalogProgram) -> TimelineBehavior {
    let descriptor = &program.definition.descriptor;
    let input_name = |slot: crate::program::InputSlot| descriptor.input(slot).name.clone();
    match program.definition.timeline_behavior {
        ProgramTimelineBehavior::Fresh => TimelineBehavior::Fresh,
        ProgramTimelineBehavior::Identity { input } => TimelineBehavior::Identity {
            input: input_name(input),
        },
        ProgramTimelineBehavior::Repeat { input } => TimelineBehavior::Repeat {
            input: input_name(input),
        },
        ProgramTimelineBehavior::Concat { input } => TimelineBehavior::Concat {
            input: input_name(input),
        },
        ProgramTimelineBehavior::BodyConcat { inputs } => TimelineBehavior::BodyConcat {
            inputs: inputs.iter().copied().map(input_name).collect(),
        },
        ProgramTimelineBehavior::Crop { input } => TimelineBehavior::Crop {
            input: input_name(input),
        },
        ProgramTimelineBehavior::Replace { base } => TimelineBehavior::Replace {
            base: input_name(base),
        },
        ProgramTimelineBehavior::FlashCut { before, after } => TimelineBehavior::FlashCut {
            before: input_name(before),
            after: input_name(after),
        },
        ProgramTimelineBehavior::Crossfade { before, after } => TimelineBehavior::Crossfade {
            before: input_name(before),
            after: input_name(after),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn public_reference_covers_every_builtin_once() {
        let programs = builtin_programs();
        assert_eq!(programs.len(), 14);
        assert_eq!(
            programs
                .iter()
                .map(BuiltinProgram::name)
                .collect::<BTreeSet<_>>()
                .len(),
            programs.len()
        );
        assert_eq!(
            programs
                .iter()
                .map(BuiltinProgram::documentation_route)
                .collect::<BTreeSet<_>>()
                .len(),
            programs.len()
        );
    }

    #[test]
    fn contextual_and_typed_defaults_remain_distinct() {
        let image = builtin_program("image").expect("image");
        assert_eq!(image.parameters[1].name, "duration");
        assert!(!image.parameters[1].required);
        assert_eq!(image.parameters[1].default, None);
        assert_eq!(
            image.parameters[2]
                .default
                .as_ref()
                .map(ToString::to_string),
            Some("cover".to_owned())
        );

        let zoom = builtin_program("zoom_in").expect("zoom_in");
        assert_eq!(
            zoom.parameters[0].default.as_ref().map(ToString::to_string),
            Some("8%".to_owned())
        );
    }
}
