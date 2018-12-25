package texlab.completion

import org.eclipse.lsp4j.Position
import texlab.Document
import java.net.URI

data class CompletionRequest(val uri: URI,
                             val relatedDocuments: List<Document>,
                             val position: Position) {
    val document: Document = relatedDocuments.first { it.uri == uri }
}
