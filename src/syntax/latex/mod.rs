mod ast;
mod finder;
mod lexer;
mod parser;

pub use self::ast::*;
use self::finder::LatexFinder;
pub use self::finder::LatexNode;
use self::lexer::LatexLexer;
use self::parser::LatexParser;
use super::language::*;
use super::text::{CharStream, SyntaxNode};
use lsp_types::{Position, Range, Uri};
use path_clean::PathClean;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Default)]
struct LatexCommandAnalyzer {
    commands: Vec<Arc<LatexCommand>>,
}

impl LatexCommandAnalyzer {
    fn parse(root: Arc<LatexRoot>) -> Vec<Arc<LatexCommand>> {
        let mut analyzer = Self::default();
        analyzer.visit_root(root);
        analyzer.commands
    }
}

impl LatexVisitor for LatexCommandAnalyzer {
    fn visit_root(&mut self, root: Arc<LatexRoot>) {
        LatexWalker::walk_root(self, root);
    }

    fn visit_group(&mut self, group: Arc<LatexGroup>) {
        LatexWalker::walk_group(self, group);
    }

    fn visit_command(&mut self, command: Arc<LatexCommand>) {
        self.commands.push(Arc::clone(&command));
        LatexWalker::walk_command(self, command);
    }

    fn visit_text(&mut self, text: Arc<LatexText>) {
        LatexWalker::walk_text(self, text);
    }

    fn visit_comma(&mut self, comma: Arc<LatexComma>) {
        LatexWalker::walk_comma(self, comma);
    }

    fn visit_math(&mut self, math: Arc<LatexMath>) {
        LatexWalker::walk_math(self, math);
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexEnvironmentDelimiter {
    pub command: Arc<LatexCommand>,
}

impl LatexEnvironmentDelimiter {
    pub fn name(&self) -> Option<&LatexToken> {
        self.command.extract_word(0)
    }

    pub fn is_math(&self) -> bool {
        if let Some(name) = self.name() {
            LANGUAGE_DATA
                .math_environments
                .iter()
                .any(|env| env == name.text())
        } else {
            false
        }
    }
}

impl SyntaxNode for LatexEnvironmentDelimiter {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexEnvironment {
    pub left: LatexEnvironmentDelimiter,
    pub right: LatexEnvironmentDelimiter,
}

impl LatexEnvironment {
    pub fn is_root(&self) -> bool {
        self.left
            .name()
            .iter()
            .chain(self.right.name().iter())
            .any(|name| name.text() == "document")
    }

    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut stack = Vec::new();
        let mut environments = Vec::new();
        for command in commands {
            if let Some(delimiter) = Self::parse_delimiter(command) {
                if delimiter.command.name.text() == "\\begin" {
                    stack.push(delimiter);
                } else if let Some(begin) = stack.pop() {
                    environments.push(Self {
                        left: begin,
                        right: delimiter,
                    });
                }
            }
        }
        environments
    }

    fn parse_delimiter(command: &Arc<LatexCommand>) -> Option<LatexEnvironmentDelimiter> {
        if command.name.text() != "\\begin" && command.name.text() != "\\end" {
            return None;
        }

        if command.args.len() == 0 {
            return None;
        }

        if command.has_word(0)
            || command.args[0].children.is_empty()
            || command.args[0].right.is_none()
        {
            let delimiter = LatexEnvironmentDelimiter {
                command: Arc::clone(&command),
            };
            Some(delimiter)
        } else {
            None
        }
    }
}

impl SyntaxNode for LatexEnvironment {
    fn range(&self) -> Range {
        Range::new(self.left.start(), self.right.end())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexCitation {
    pub command: Arc<LatexCommand>,
    pub index: usize,
}

impl LatexCitation {
    pub fn keys(&self) -> Vec<&LatexToken> {
        self.command.extract_comma_separated_words(0)
    }

    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut citations = Vec::new();
        for command in commands {
            for LatexCitationCommand { name, index } in &LANGUAGE_DATA.citation_commands {
                if command.name.text() == name && command.has_comma_separated_words(*index) {
                    citations.push(Self {
                        command: Arc::clone(command),
                        index: *index,
                    });
                }
            }
        }
        citations
    }
}

impl SyntaxNode for LatexCitation {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexLabel {
    pub command: Arc<LatexCommand>,
    index: usize,
    pub kind: LatexLabelKind,
}

impl LatexLabel {
    pub fn names(&self) -> Vec<&LatexToken> {
        self.command.extract_comma_separated_words(self.index)
    }

    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut labels = Vec::new();
        for command in commands {
            for LatexLabelCommand { name, index, kind } in &LANGUAGE_DATA.label_commands {
                if command.name.text() == name && command.has_comma_separated_words(*index) {
                    labels.push(Self {
                        command: Arc::clone(command),
                        index: *index,
                        kind: *kind,
                    });
                }
            }
        }
        labels
    }
}

impl SyntaxNode for LatexLabel {
    fn range(&self) -> Range {
        self.command.range
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexSection {
    pub command: Arc<LatexCommand>,
    pub index: usize,
    pub level: i32,
}

impl LatexSection {
    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut sections = Vec::new();
        for command in commands {
            for LatexSectionCommand { name, index, level } in &LANGUAGE_DATA.section_commands {
                if command.name.text() == name && command.args.len() > *index {
                    sections.push(Self {
                        command: Arc::clone(command),
                        index: *index,
                        level: *level,
                    })
                }
            }
        }
        sections
    }

    pub fn extract_text(&self, text: &str) -> Option<String> {
        let content = &self.command.args[self.index];
        let right = content.right.as_ref()?;
        let range = Range::new_simple(
            content.left.start().line,
            content.left.start().character + 1,
            right.end().line,
            right.end().character - 1,
        );
        Some(CharStream::extract(&text, range))
    }
}

impl SyntaxNode for LatexSection {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexInclude {
    pub command: Arc<LatexCommand>,
    pub index: usize,
    pub kind: LatexIncludeKind,
    pub all_targets: Vec<Vec<Uri>>,
    pub include_extension: bool,
}

impl LatexInclude {
    pub fn paths(&self) -> Vec<&LatexToken> {
        self.command.extract_comma_separated_words(self.index)
    }

    pub fn components(&self) -> Vec<String> {
        let mut components = Vec::new();
        for path in self.paths() {
            match self.kind {
                LatexIncludeKind::Package => components.push(format!("{}.sty", path.text())),
                LatexIncludeKind::Class => components.push(format!("{}.cls", path.text())),
                LatexIncludeKind::Latex
                | LatexIncludeKind::Bibliography
                | LatexIncludeKind::Image
                | LatexIncludeKind::Svg
                | LatexIncludeKind::Pdf
                | LatexIncludeKind::Everything => (),
            }
        }
        components
    }

    fn parse(uri: &Uri, commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut includes = Vec::new();
        for command in commands {
            for description in &LANGUAGE_DATA.include_commands {
                if let Some(include) = Self::parse_single(uri, &command, &description) {
                    includes.push(include);
                }
            }
        }
        includes
    }

    fn parse_single(
        uri: &Uri,
        command: &Arc<LatexCommand>,
        description: &LatexIncludeCommand,
    ) -> Option<Self> {
        if command.name.text() != description.name {
            return None;
        }

        if command.args.len() <= description.index {
            return None;
        }

        let mut all_targets = Vec::new();
        for relative_path in command.extract_comma_separated_words(description.index) {
            let mut path = uri.to_file_path().ok()?;
            path.pop();
            path.push(relative_path.text());
            path = PathBuf::from(path.to_string_lossy().into_owned().replace('\\', "/"));
            path = path.clean();
            let path = path.to_str()?.to_owned();

            let mut targets = Vec::new();
            targets.push(Uri::from_file_path(&path).ok()?);
            if let Some(extensions) = description.kind.extensions() {
                for extension in extensions {
                    let path = format!("{}.{}", &path, extension);
                    targets.push(Uri::from_file_path(&path).ok()?);
                }
            }
            all_targets.push(targets);
        }

        let include = Self {
            command: Arc::clone(command),
            index: description.index,
            kind: description.kind,
            all_targets,
            include_extension: description.include_extension,
        };
        Some(include)
    }
}

impl SyntaxNode for LatexInclude {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexEquation {
    pub left: Arc<LatexCommand>,
    pub right: Arc<LatexCommand>,
}

impl LatexEquation {
    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut equations = Vec::new();
        let mut left = None;
        for command in commands {
            let name = command.name.text();
            if name == "\\[" || name == "\\(" {
                left = Some(command);
            } else if name == "\\]" || name == "\\)" {
                if let Some(begin) = left {
                    equations.push(Self {
                        left: Arc::clone(&begin),
                        right: Arc::clone(&command),
                    });
                    left = None;
                }
            }
        }
        equations
    }
}

impl SyntaxNode for LatexEquation {
    fn range(&self) -> Range {
        Range::new(self.left.start(), self.right.end())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexInline {
    pub left: Arc<LatexMath>,
    pub right: Arc<LatexMath>,
}

impl LatexInline {
    fn parse(root: Arc<LatexRoot>) -> Vec<Self> {
        let mut analyzer = LatexInlineAnalyzer::default();
        analyzer.visit_root(root);
        analyzer.inlines
    }
}

impl SyntaxNode for LatexInline {
    fn range(&self) -> Range {
        Range::new(self.left.start(), self.right.end())
    }
}

#[derive(Debug, Default)]
struct LatexInlineAnalyzer {
    inlines: Vec<LatexInline>,
    left: Option<Arc<LatexMath>>,
}

impl LatexVisitor for LatexInlineAnalyzer {
    fn visit_root(&mut self, root: Arc<LatexRoot>) {
        LatexWalker::walk_root(self, root);
    }

    fn visit_group(&mut self, group: Arc<LatexGroup>) {
        LatexWalker::walk_group(self, group);
    }

    fn visit_command(&mut self, command: Arc<LatexCommand>) {
        LatexWalker::walk_command(self, command);
    }

    fn visit_text(&mut self, text: Arc<LatexText>) {
        LatexWalker::walk_text(self, text);
    }

    fn visit_comma(&mut self, comma: Arc<LatexComma>) {
        LatexWalker::walk_comma(self, comma);
    }

    fn visit_math(&mut self, math: Arc<LatexMath>) {
        if let Some(left) = &self.left {
            let inline = LatexInline {
                left: Arc::clone(&left),
                right: Arc::clone(&math),
            };
            self.inlines.push(inline);
            self.left = None;
        } else {
            self.left = Some(Arc::clone(&math));
        }
        LatexWalker::walk_math(self, math);
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexMathOperator {
    pub command: Arc<LatexCommand>,
    pub definition: Arc<LatexCommand>,
    pub definition_index: usize,
    pub implementation_index: usize,
}

impl LatexMathOperator {
    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut operators = Vec::new();
        for command in commands {
            for LatexMathOperatorCommand {
                name,
                definition_index,
                implementation_index,
            } in &LANGUAGE_DATA.math_operator_commands
            {
                if command.name.text() == name
                    && command.args.len() > *definition_index
                    && command.args.len() > *implementation_index
                {
                    let definition = command.args[0].children.iter().next();
                    if let Some(LatexContent::Command(definition)) = definition {
                        operators.push(Self {
                            command: Arc::clone(command),
                            definition: Arc::clone(definition),
                            definition_index: *definition_index,
                            implementation_index: *implementation_index,
                        })
                    }
                }
            }
        }
        operators
    }
}

impl SyntaxNode for LatexMathOperator {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexCommandDefinition {
    pub command: Arc<LatexCommand>,
    pub definition: Arc<LatexCommand>,
    pub definition_index: usize,
    pub implementation: Arc<LatexGroup>,
    pub implementation_index: usize,
    pub argument_count_index: usize,
}

impl LatexCommandDefinition {
    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut definitions = Vec::new();
        for command in commands {
            for LatexCommandDefinitionCommand {
                name,
                definition_index,
                argument_count_index,
                implementation_index,
            } in &LANGUAGE_DATA.command_definition_commands
            {
                if command.name.text() == name
                    && command.args.len() > *definition_index
                    && command.args.len() > *implementation_index
                {
                    let definition = command.args[0].children.iter().next();
                    if let Some(LatexContent::Command(definition)) = definition {
                        definitions.push(Self {
                            command: Arc::clone(command),
                            definition: Arc::clone(definition),
                            definition_index: *definition_index,
                            implementation: Arc::clone(&command.args[*implementation_index]),
                            implementation_index: *implementation_index,
                            argument_count_index: *argument_count_index,
                        })
                    }
                }
            }
        }
        definitions
    }
}

impl SyntaxNode for LatexCommandDefinition {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexTheoremDefinition {
    pub command: Arc<LatexCommand>,
    pub index: usize,
}

impl LatexTheoremDefinition {
    pub fn name(&self) -> &LatexToken {
        self.command.extract_word(self.index).unwrap()
    }

    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut definitions = Vec::new();
        for command in commands {
            for LatexTheoremDefinitionCommand { name, index } in
                &LANGUAGE_DATA.theorem_definition_commands
            {
                if command.name.text() == name && command.has_word(*index) {
                    definitions.push(Self {
                        command: Arc::clone(&command),
                        index: *index,
                    });
                }
            }
        }
        definitions
    }
}

impl SyntaxNode for LatexTheoremDefinition {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexCaption {
    pub command: Arc<LatexCommand>,
    pub index: usize,
}

impl LatexCaption {
    fn parse(commands: &[Arc<LatexCommand>]) -> Vec<Self> {
        let mut captions = Vec::new();
        for command in commands {
            if command.name.text() == "\\caption" && command.args.len() > 0 {
                captions.push(Self {
                    command: Arc::clone(&command),
                    index: 0,
                });
            }
        }
        captions
    }
}

impl SyntaxNode for LatexCaption {
    fn range(&self) -> Range {
        self.command.range()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LatexSyntaxTree {
    pub root: Arc<LatexRoot>,
    pub commands: Vec<Arc<LatexCommand>>,
    pub includes: Vec<LatexInclude>,
    pub components: Vec<String>,
    pub environments: Vec<LatexEnvironment>,
    pub is_standalone: bool,
    pub labels: Vec<LatexLabel>,
    pub sections: Vec<LatexSection>,
    pub citations: Vec<LatexCitation>,
    pub equations: Vec<LatexEquation>,
    pub inlines: Vec<LatexInline>,
    pub math_operators: Vec<LatexMathOperator>,
    pub command_definitions: Vec<LatexCommandDefinition>,
    pub theorem_definitions: Vec<LatexTheoremDefinition>,
    pub captions: Vec<LatexCaption>,
}

impl LatexSyntaxTree {
    pub fn parse(uri: &Uri, text: &str) -> Self {
        let lexer = LatexLexer::new(text);
        let mut parser = LatexParser::new(lexer);
        let root = Arc::new(parser.root());
        let commands = LatexCommandAnalyzer::parse(Arc::clone(&root));
        let includes = LatexInclude::parse(uri, &commands);
        let components = includes.iter().flat_map(LatexInclude::components).collect();
        let environments = LatexEnvironment::parse(&commands);
        let is_standalone = environments.iter().any(LatexEnvironment::is_root);
        let labels = LatexLabel::parse(&commands);
        let sections = LatexSection::parse(&commands);
        let citations = LatexCitation::parse(&commands);
        let equations = LatexEquation::parse(&commands);
        let inlines = LatexInline::parse(Arc::clone(&root));
        let math_operators = LatexMathOperator::parse(&commands);
        let command_definitions = LatexCommandDefinition::parse(&commands);
        let theorem_definitions = LatexTheoremDefinition::parse(&commands);
        let captions = LatexCaption::parse(&commands);
        Self {
            root,
            commands,
            includes,
            components,
            environments,
            is_standalone,
            labels,
            sections,
            citations,
            equations,
            inlines,
            math_operators,
            command_definitions,
            theorem_definitions,
            captions,
        }
    }

    pub fn find(&self, position: Position) -> Vec<LatexNode> {
        let mut finder = LatexFinder::new(position);
        finder.visit_root(Arc::clone(&self.root));
        finder.results
    }

    pub fn find_command_by_name(&self, position: Position) -> Option<Arc<LatexCommand>> {
        for result in self.find(position) {
            if let LatexNode::Command(command) = result {
                if command.name.range().contains(position)
                    && command.name.start().character != position.character
                {
                    return Some(command);
                }
            }
        }
        None
    }
}
