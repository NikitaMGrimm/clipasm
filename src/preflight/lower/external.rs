use crate::diagnostic::Result;
use crate::external::{ExternalArgumentValue, ExternalInvocation, ExternalParameterValue};
use crate::model::NodeId;
use crate::semantic::CompiledNode;

use super::super::assets::prepare_external_file_asset;
use super::super::tools::inspect_external_tool;
use super::super::{PreparedExternalArgument, PreparedExternalParameterValue, PreparedVideoKind};
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
    let preserved_domain = *preserved_domain;
    let executable =
        inspect_external_tool(&invocation.executable.value, &invocation.executable.span)?;
    let arguments = invocation
        .arguments
        .iter()
        .map(|argument| match argument {
            ExternalArgumentValue::Text { value } => {
                Ok(PreparedExternalArgument::Text(value.clone()))
            }
            ExternalArgumentValue::File { path } => Ok(PreparedExternalArgument::File(
                prepare_external_file_asset(&path.value, &path.span)?,
            )),
        })
        .collect::<Result<Vec<_>>>()?;
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
            arguments,
            inputs,
            parameters,
            preserve_input: invocation.preserve_input.clone(),
        },
        preserved_domain,
        preserved_has_audio,
        node.semantic_version(),
        node.origin().clone(),
    )
}
