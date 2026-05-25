package dev.mobiler.abi

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.mobiler.abi.ui.theme.AbiTheme
import dev.mobiler.abi.shared.types.Action
import dev.mobiler.abi.shared.types.InputValue
import dev.mobiler.abi.shared.types.Widget

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AbiTheme {
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
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Render(core.view) { action -> core.update(action) }
    }
}

/**
 * The ENTIRE shell. It knows only the fixed Mobiler ABI — `Widget` (what to
 * draw) and `Action` (what to send back). There are NO app-specific types here:
 * this exact code renders any Mobiler app. Action tokens are opaque Strings the
 * shell round-trips without interpreting.
 */
@Composable
fun Render(widget: Widget, send: (Action) -> Unit) {
    when (widget) {
        is Widget.Text -> Text(text = widget.content)

        is Widget.Column -> Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            widget.children.forEach { child -> Render(child, send) }
        }

        is Widget.Button -> Button(onClick = { send(Action.Fired(token = widget.onPress)) }) {
            Text(text = widget.label)
        }

        is Widget.TextField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { newValue ->
                send(Action.Input(id = widget.id, value = InputValue.Text(newValue)))
            },
            placeholder = { Text(widget.placeholder) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}
