package {{PACKAGE}}

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import {{PACKAGE}}.ui.theme.{{NAME}}Theme
import {{PACKAGE_SHARED_TYPES}}.Event
import {{PACKAGE_SHARED_TYPES}}.Widget

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            {{NAME}}Theme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    App()
                }
            }
        }
    }
}

@Composable
fun App(core: Core = viewModel()) {
    Render(core.view) { event -> core.update(event) }
}

/**
 * Generic Render — knows nothing about the app, just maps Widget variants to
 * Compose primitives. Adding a new screen means writing Rust; this file only
 * changes when a brand-new Widget kind is added to the framework.
 */
@Composable
fun Render(widget: Widget, send: (Event) -> Unit) {
    when (widget) {
        is Widget.Text -> Text(text = widget.content, modifier = Modifier.padding(8.dp))
        is Widget.Button -> Button(onClick = { send(widget.onPress) }) {
            Text(text = widget.label)
        }
        is Widget.Column -> Column(
            modifier = Modifier.fillMaxSize().padding(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            widget.children.forEach { child -> Render(child, send) }
        }
        is Widget.AlertDialog -> AlertDialog(
            onDismissRequest = { send(widget.onDismiss) },
            text = { Text(text = widget.message) },
            confirmButton = {
                TextButton(onClick = { send(widget.onDismiss) }) { Text("OK") }
            },
        )
    }
}
