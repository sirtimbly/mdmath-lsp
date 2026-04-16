mod analysis;
mod lang;
mod markdown;
mod text;

use std::collections::HashMap;

use analysis::Analysis;
use text::{apply_content_changes, position_to_offset, range_overlaps, span_to_range};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct DocumentState {
    text: String,
    analysis: Analysis,
}

struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, DocumentState>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
        }
    }

    async fn update_document(&self, uri: Url, text: String) {
        let analysis = analysis::analyze(&text);
        let diagnostics = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.to_lsp(&text))
            .collect();

        self.documents
            .write()
            .await
            .insert(uri.clone(), DocumentState { text, analysis });

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "mdmath-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "mdmath-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        let current_text = {
            let documents = self.documents.read().await;
            documents
                .get(&uri)
                .map(|doc| doc.text.clone())
                .unwrap_or_default()
        };

        let next_text = apply_content_changes(current_text, params.content_changes);
        self.update_document(uri, next_text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let documents = self.documents.read().await;
        let Some(document) = documents.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&document.text, position);
        let Some(statement) = document.analysis.statement_at_offset(offset) else {
            return Ok(None);
        };
        let Some(contents) = statement.hover_text() else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: contents,
            }),
            range: Some(span_to_range(&document.text, statement.source_span())),
        }))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let documents = self.documents.read().await;
        let Some(document) = documents.get(&params.text_document.uri) else {
            return Ok(Some(Vec::new()));
        };

        let hints = document
            .analysis
            .statements()
            .iter()
            .filter_map(|statement| {
                let label = statement.hint_label()?;
                let hint_range = span_to_range(&document.text, statement.display_span());
                if !range_overlaps(&hint_range, &params.range) {
                    return None;
                }

                Some(InlayHint {
                    position: hint_range.end,
                    label: InlayHintLabel::String(format!(" {label}")),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: statement.hover_text().map(|text| {
                        InlayHintTooltip::MarkupContent(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: text,
                        })
                    }),
                    padding_left: Some(true),
                    padding_right: Some(false),
                    data: None,
                })
            })
            .collect();

        Ok(Some(hints))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let documents = self.documents.read().await;
        let Some(document) = documents.get(&params.text_document.uri) else {
            return Ok(None);
        };

        let uri = params.text_document.uri.clone();
        let mut actions = Vec::new();

        for statement in document.analysis.statements() {
            if !range_overlaps(
                &span_to_range(&document.text, statement.display_span()),
                &params.range,
            ) {
                continue;
            }

            let Some(replacement) = statement.replacement_text() else {
                continue;
            };

            let replace = CodeAction {
                title: format!("Replace with {replacement}"),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(
                        uri.clone(),
                        vec![TextEdit {
                            range: span_to_range(&document.text, statement.source_span()),
                            new_text: replacement.clone(),
                        }],
                    )])),
                    ..WorkspaceEdit::default()
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(replace));

            let insertion = CodeAction {
                title: format!(
                    "Insert evaluated result ({})",
                    statement.hint_label().unwrap()
                ),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range {
                                start: span_to_range(&document.text, statement.insert_span()).end,
                                end: span_to_range(&document.text, statement.insert_span()).end,
                            },
                            new_text: format!(" {}", statement.hint_label().unwrap()),
                        }],
                    )])),
                    ..WorkspaceEdit::default()
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(insertion));
        }

        Ok(Some(actions))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
