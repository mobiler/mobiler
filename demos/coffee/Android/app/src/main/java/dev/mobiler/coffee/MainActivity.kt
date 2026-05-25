package dev.mobiler.coffee

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import coil3.compose.AsyncImage
import dev.mobiler.coffee.ui.theme.CoffeeTheme
import dev.mobiler.coffee.shared.types.BoxAlign
import dev.mobiler.coffee.shared.types.Event
import dev.mobiler.coffee.shared.types.ImageRatio
import dev.mobiler.coffee.shared.types.ImageShape
import dev.mobiler.coffee.shared.types.Widget

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge() // transparent system bars + icon contrast that adapts to light/dark
        setContent {
            CoffeeTheme {
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
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Render(core.view) { event -> core.update(event) }
    }
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
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            widget.children.forEach { child -> Render(child, send) }
        }
        is Widget.Row -> Row(
            modifier = Modifier.horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            widget.children.forEach { child -> Render(child, send) }
        }
        is Widget.Card -> {
            val onPress = widget.onPress
            if (onPress != null) {
                Card(onClick = { send(onPress) }, modifier = Modifier.fillMaxWidth()) {
                    Box(modifier = Modifier.padding(8.dp)) { Render(widget.child, send) }
                }
            } else {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Box(modifier = Modifier.padding(8.dp)) { Render(widget.child, send) }
                }
            }
        }
        is Widget.Chip -> FilterChip(
            selected = widget.selected,
            onClick = { send(widget.onPress) },
            label = { Text(widget.label) },
        )
        is Widget.Slider -> Slider(
            value = widget.value.toFloat(),
            onValueChange = { send(Event.SliderChanged(id = widget.id, value = it.toInt())) },
            valueRange = 0f..widget.max.toFloat(),
            modifier = Modifier.fillMaxWidth(),
        )
        is Widget.Stepper -> Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OutlinedButton(onClick = { send(widget.onDecrement) }) { Text("−") }
            Text(text = "${widget.value}", style = MaterialTheme.typography.titleMedium)
            OutlinedButton(onClick = { send(widget.onIncrement) }) { Text("+") }
        }
        is Widget.Grid -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            widget.children.chunked(2).forEach { rowItems ->
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    rowItems.forEach { item ->
                        Box(modifier = Modifier.weight(1f)) { Render(item, send) }
                    }
                    repeat(2 - rowItems.size) { Spacer(modifier = Modifier.weight(1f)) }
                }
            }
        }
        is Widget.Image -> AsyncImage(
            model = widget.source,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(ratioFor(widget.ratio))
                .clip(shapeFor(widget.shape)),
        )
        is Widget.Box -> Box(contentAlignment = boxAlignFor(widget.align)) {
            val kids = widget.children
            if (widget.scrim && kids.size > 1) {
                Render(kids.first(), send) // background (e.g. hero image)
                Box(Modifier.matchParentSize().background(Color.Black.copy(alpha = 0.40f)))
                // Overlaid content reads on a dark scrim → light content color.
                CompositionLocalProvider(LocalContentColor provides Color.White) {
                    Column(modifier = Modifier.fillMaxWidth().padding(16.dp)) {
                        kids.drop(1).forEach { child -> Render(child, send) }
                    }
                }
            } else {
                kids.forEach { child -> Render(child, send) }
            }
        }
    }
}

private fun shapeFor(shape: ImageShape): Shape = when (shape) {
    ImageShape.SQUARE -> RectangleShape
    ImageShape.ROUNDED -> RoundedCornerShape(16.dp)
    ImageShape.CIRCLE -> CircleShape
}

private fun ratioFor(ratio: ImageRatio): Float = when (ratio) {
    ImageRatio.WIDE -> 16f / 10f
    ImageRatio.SQUARE -> 1f
    ImageRatio.TALL -> 3f / 4f
}

private fun boxAlignFor(align: BoxAlign): Alignment = when (align) {
    BoxAlign.TOPSTART -> Alignment.TopStart
    BoxAlign.TOPEND -> Alignment.TopEnd
    BoxAlign.CENTER -> Alignment.Center
    BoxAlign.BOTTOMSTART -> Alignment.BottomStart
    BoxAlign.BOTTOMCENTER -> Alignment.BottomCenter
    BoxAlign.BOTTOMEND -> Alignment.BottomEnd
}
