package dev.mobiler.coffee

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import coil3.compose.AsyncImage
import dev.mobiler.coffee.ui.theme.CoffeeTheme
import dev.mobiler.coffee.shared.types.Action
import dev.mobiler.coffee.shared.types.BoxAlign
import dev.mobiler.coffee.shared.types.ButtonStyle
import dev.mobiler.coffee.shared.types.CardStyle
import dev.mobiler.coffee.shared.types.Icon as WidgetIcon
import dev.mobiler.coffee.shared.types.ImageRatio
import dev.mobiler.coffee.shared.types.ImageShape
import dev.mobiler.coffee.shared.types.InputValue
import dev.mobiler.coffee.shared.types.Spacing
import dev.mobiler.coffee.shared.types.TextStyle as ModelTextStyle
import dev.mobiler.coffee.shared.types.Tone
import dev.mobiler.coffee.shared.types.Widget

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { App() }
    }
}

@Composable
fun App(core: Core = viewModel()) {
    val view = core.view
    // Theme is data: a Scaffold carries dark_mode, decided by the Rust core.
    val dark = (view as? Widget.Scaffold)?.darkMode ?: isSystemInDarkTheme()
    CoffeeTheme(darkTheme = dark) {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            if (view is Widget.Scaffold) {
                // Scaffold provides its own bars + scrollable body.
                Render(view) { action -> core.update(action) }
            } else {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .verticalScroll(rememberScrollState())
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Render(view) { action -> core.update(action) }
                }
            }
        }
    }
}

/**
 * The ENTIRE shell. It knows only the fixed Mobiler ABI — `Widget` (what to
 * draw) + `Action` (what to send back). No app-specific types; this exact code
 * renders any Mobiler app. Style *intent* (TextStyle, Tone, …) is decided in
 * Rust; the concrete look (fonts, colors, dp) is decided here.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Render(widget: Widget, send: (Action) -> Unit) {
    when (widget) {
        is Widget.Text -> Text(
            text = widget.content,
            style = typographyFor(widget.style),
            fontWeight = if (widget.style == ModelTextStyle.EMPHASIS) FontWeight.Medium else null,
            color = colorFor(widget.style),
            modifier = Modifier.padding(vertical = 2.dp),
        )

        is Widget.Image -> AsyncImage(
            model = widget.source,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.fillMaxWidth().aspectRatio(ratioFor(widget.ratio)).clip(shapeFor(widget.shape)),
        )

        is Widget.Badge -> {
            val (bg, fg) = toneColors(widget.tone)
            Box(
                modifier = Modifier.background(color = bg, shape = RoundedCornerShape(50)).padding(horizontal = 12.dp, vertical = 4.dp),
            ) { Text(text = widget.label, style = MaterialTheme.typography.labelMedium, color = fg) }
        }

        is Widget.Divider -> HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

        is Widget.Spacer -> Spacer(modifier = Modifier.height(spacingFor(widget.size)))

        is Widget.Row -> Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) { widget.children.forEach { Render(it, send) } }

        is Widget.Column -> Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) { widget.children.forEach { Render(it, send) } }

        is Widget.Card -> {
            val mod = Modifier.fillMaxWidth()
            val op = widget.onPress
            when (widget.style) {
                CardStyle.OUTLINED ->
                    if (op != null) OutlinedCard(onClick = { send(Action.Fired(op)) }, modifier = mod) { CardBody(widget.child, send) }
                    else OutlinedCard(modifier = mod) { CardBody(widget.child, send) }
                CardStyle.FILLED -> {
                    val colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)
                    if (op != null) Card(onClick = { send(Action.Fired(op)) }, modifier = mod, colors = colors) { CardBody(widget.child, send) }
                    else Card(modifier = mod, colors = colors) { CardBody(widget.child, send) }
                }
                CardStyle.ELEVATED -> {
                    val elev = CardDefaults.cardElevation(defaultElevation = 1.dp)
                    if (op != null) Card(onClick = { send(Action.Fired(op)) }, modifier = mod, elevation = elev) { CardBody(widget.child, send) }
                    else Card(modifier = mod, elevation = elev) { CardBody(widget.child, send) }
                }
            }
        }

        is Widget.Box -> Box(contentAlignment = boxAlignFor(widget.align)) {
            val kids = widget.children
            if (widget.scrim && kids.size > 1) {
                Render(kids.first(), send)
                Box(Modifier.matchParentSize().background(Color.Black.copy(alpha = 0.40f)))
                CompositionLocalProvider(LocalContentColor provides Color.White) {
                    Column(modifier = Modifier.fillMaxWidth().padding(16.dp)) { kids.drop(1).forEach { Render(it, send) } }
                }
            } else {
                kids.forEach { Render(it, send) }
            }
        }

        is Widget.Grid -> Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            widget.children.chunked(2).forEach { rowItems ->
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    rowItems.forEach { item -> Box(modifier = Modifier.weight(1f)) { Render(item, send) } }
                    repeat(2 - rowItems.size) { Spacer(modifier = Modifier.weight(1f)) }
                }
            }
        }

        is Widget.Button -> when (widget.style) {
            ButtonStyle.FILLED -> Button(onClick = { send(Action.Fired(widget.onPress)) }) { Text(widget.label) }
            ButtonStyle.OUTLINED -> OutlinedButton(onClick = { send(Action.Fired(widget.onPress)) }) { Text(widget.label) }
            ButtonStyle.TEXT -> TextButton(onClick = { send(Action.Fired(widget.onPress)) }) { Text(widget.label) }
        }

        is Widget.IconButton -> IconButton(onClick = { send(Action.Fired(widget.onPress)) }) {
            Icon(imageVector = iconFor(widget.icon), contentDescription = widget.icon.name.lowercase(), tint = iconTintFor(widget.icon))
        }

        is Widget.Chip -> FilterChip(
            selected = widget.selected,
            onClick = { send(Action.Fired(widget.onPress)) },
            label = { Text(widget.label) },
        )

        is Widget.TextField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { send(Action.Input(widget.id, InputValue.Text(it))) },
            placeholder = { Text(widget.placeholder) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        is Widget.Switch -> Row(
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(text = widget.label, modifier = Modifier.weight(1f))
            Switch(checked = widget.value, onCheckedChange = { send(Action.Input(widget.id, InputValue.Bool(it))) })
        }

        is Widget.Checkbox -> Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Checkbox(checked = widget.value, onCheckedChange = { send(Action.Input(widget.id, InputValue.Bool(it))) })
            Text(text = widget.label, modifier = Modifier.weight(1f))
        }

        is Widget.Slider -> Slider(
            value = widget.value.toFloat(),
            onValueChange = { send(Action.Input(widget.id, InputValue.Int(it.toLong()))) },
            valueRange = 0f..widget.max.toFloat(),
            modifier = Modifier.fillMaxWidth(),
        )

        is Widget.Stepper -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedButton(onClick = { send(Action.Fired(widget.onDecrement)) }) { Text("−") }
            Text(text = "${widget.value}", style = MaterialTheme.typography.titleMedium)
            OutlinedButton(onClick = { send(Action.Fired(widget.onIncrement)) }) { Text("+") }
        }

        is Widget.Scaffold -> Scaffold(
            modifier = Modifier.fillMaxSize(),
            topBar = {
                CenterAlignedTopAppBar(
                    title = { Text(widget.title) },
                    navigationIcon = {
                        val back = widget.back
                        if (back != null) {
                            IconButton(onClick = { send(Action.Fired(back)) }) {
                                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                            }
                        }
                    },
                    colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
                )
            },
            bottomBar = {
                if (widget.tabs.isNotEmpty()) {
                    NavigationBar {
                        widget.tabs.forEach { t ->
                            NavigationBarItem(
                                selected = t.selected,
                                onClick = { send(Action.Fired(t.onSelect)) },
                                label = { Text(t.label) },
                                icon = {},
                            )
                        }
                    }
                }
            },
        ) { padding ->
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(horizontal = 16.dp)
                    .verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Render(widget.body, send)
            }
        }
    }
}

// ---------- Style-token mappings (the only place that decides concrete looks) ----------

@Composable
private fun typographyFor(style: ModelTextStyle): androidx.compose.ui.text.TextStyle = when (style) {
    ModelTextStyle.BODY -> MaterialTheme.typography.bodyLarge
    ModelTextStyle.TITLE -> MaterialTheme.typography.headlineMedium
    ModelTextStyle.SUBTITLE -> MaterialTheme.typography.titleMedium
    ModelTextStyle.CAPTION -> MaterialTheme.typography.bodySmall
    ModelTextStyle.EMPHASIS -> MaterialTheme.typography.bodyLarge
}

@Composable
private fun colorFor(style: ModelTextStyle): Color = when (style) {
    ModelTextStyle.CAPTION -> MaterialTheme.colorScheme.onSurfaceVariant
    else -> MaterialTheme.colorScheme.onSurface
}

private fun spacingFor(size: Spacing): Dp = when (size) {
    Spacing.XS -> 4.dp
    Spacing.SM -> 8.dp
    Spacing.MD -> 12.dp
    Spacing.LG -> 16.dp
    Spacing.XL -> 24.dp
}

private fun iconFor(icon: WidgetIcon): androidx.compose.ui.graphics.vector.ImageVector = when (icon) {
    WidgetIcon.DELETE -> Icons.Default.Delete
    WidgetIcon.ADD -> Icons.Default.Add
    WidgetIcon.EDIT -> Icons.Default.Edit
    WidgetIcon.CLOSE -> Icons.Default.Close
    WidgetIcon.SETTINGS -> Icons.Default.Settings
    WidgetIcon.CHECK -> Icons.Default.Check
    WidgetIcon.STAR -> Icons.Default.Star
}

@Composable
private fun iconTintFor(icon: WidgetIcon): Color = when (icon) {
    WidgetIcon.STAR -> MaterialTheme.colorScheme.primary
    else -> LocalContentColor.current
}

@Composable
private fun toneColors(tone: Tone): Pair<Color, Color> {
    val cs = MaterialTheme.colorScheme
    return when (tone) {
        Tone.NEUTRAL -> cs.surfaceVariant to cs.onSurfaceVariant
        Tone.SUCCESS -> Color(0xFF2E7D32).copy(alpha = 0.15f) to Color(0xFF2E7D32)
        Tone.WARNING -> Color(0xFFE65100).copy(alpha = 0.15f) to Color(0xFFE65100)
        Tone.DANGER -> cs.errorContainer to cs.onErrorContainer
        Tone.INFO -> cs.tertiaryContainer to cs.onTertiaryContainer
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

@Composable
private fun CardBody(child: Widget, send: (Action) -> Unit) {
    Box(modifier = Modifier.padding(16.dp)) { Render(child, send) }
}
