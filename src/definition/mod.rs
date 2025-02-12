mod latex_citation;
mod latex_command;
mod latex_label;

use self::latex_citation::LatexCitationDefinitionProvider;
use self::latex_command::LatexCommandDefinitionProvider;
use self::latex_label::LatexLabelDefinitionProvider;
use crate::workspace::*;
use futures_boxed::boxed;
use lsp_types::{Location, LocationLink, TextDocumentPositionParams};
use serde::{Deserialize, Serialize};

pub struct DefinitionProvider {
    provider: ConcatProvider<TextDocumentPositionParams, LocationLink>,
}

impl DefinitionProvider {
    pub fn new() -> Self {
        Self {
            provider: ConcatProvider::new(vec![
                Box::new(LatexCitationDefinitionProvider),
                Box::new(LatexCommandDefinitionProvider),
                Box::new(LatexLabelDefinitionProvider),
            ]),
        }
    }
}

impl FeatureProvider for DefinitionProvider {
    type Params = TextDocumentPositionParams;
    type Output = Vec<LocationLink>;

    #[boxed]
    async fn execute<'a>(&'a self, request: &'a FeatureRequest<Self::Params>) -> Self::Output {
        self.provider.execute(request).await
    }
}

#[serde(untagged)]
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum DefinitionResponse {
    Locations(Vec<Location>),
    LocationLinks(Vec<LocationLink>),
}
