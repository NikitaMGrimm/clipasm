use crate::diagnostic::Result;
use crate::external::{ExternalInvocation, ExternalParameterValue};
use crate::model::NodeId;
use crate::semantic::CompiledNode;

use super::super::assets::prepare_external_file_asset;
use super::super::tools::inspect_external_tool;
use super::super::{PreparedExternalParameterValue, PreparedVideoKind};
use super::PreflightLowerer;

pub(super) fn video(
    lowerer: &mut PreflightLowerer<'_>,
    node: &CompiledNode,
    invocation: &ExternalInvocation,
) -> Result<NodeId> {
    let inputs = invocation
        .inputs
        .iter()
        .map(|(name, input)| {
            lowerer
                .prepared_dependency(*input, node.origin())
                .map(|input| (name.clone(), input))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    let preserved = inputs[&invocation.preserve_input];
    let (preserved_domain, preserved_has_audio) = lowerer.video_domain(preserved, node.origin())?;
    let executable = inspect_external_tool(&invocation.command.value, &invocation.command.span)?;
    let parameters = invocation
        .parameters
        .iter()
        .map(|(name, value)| {
            let value = match value {
                ExternalParameterValue::Integer(value) => {
                    PreparedExternalParameterValue::Integer(*value)
                }
                ExternalParameterValue::Keyword(value) => {
                    PreparedExternalParameterValue::Keyword(value.clone())
                }
                ExternalParameterValue::File(path) => PreparedExternalParameterValue::File(
                    prepare_external_file_asset(&path.value, &path.span)?,
                ),
            };
            Ok((name.clone(), value))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    lowerer.add_video_node(
        PreparedVideoKind::ExternalVideo {
            executable,
            inputs,
            parameters,
            preserve_input: invocation.preserve_input.clone(),
        },
        *preserved_domain,
        preserved_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}
