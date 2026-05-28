package dev.mobiler.coffee

import android.net.Uri
import android.os.Bundle
import androidx.core.content.FileProvider
import java.io.File
import java.lang.ref.WeakReference
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
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
import dev.mobiler.coffee.shared.types.ProjectColor
import dev.mobiler.coffee.shared.types.Spacing
import dev.mobiler.coffee.shared.types.TextStyle as ModelTextStyle
import dev.mobiler.coffee.shared.types.Tone
import dev.mobiler.coffee.shared.types.Widget

class MainActivity : ComponentActivity() {
    private var pendingPhoto: ((String?) -> Unit)? = null
    // The photo picker's result launcher must be registered on the Activity (before
    // it's STARTED), so the photo capability is wired to it via the PhotoPicker holder.
    private val pickMedia = registerForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        pendingPhoto?.invoke(uri?.toString())
        pendingPhoto = null
    }

    // Camera capture (cx.capture_photo): TakePicture writes the full photo to a
    // FileProvider URI we supply and reports success; we hand that URI back. It launches
    // the system camera app, so no CAMERA permission is required.
    private var pendingCamera: ((String?) -> Unit)? = null
    private var pendingCameraUri: Uri? = null
    private val takePicture = registerForActivityResult(ActivityResultContracts.TakePicture()) { success ->
        pendingCamera?.invoke(if (success) pendingCameraUri?.toString() else null)
        pendingCamera = null
        pendingCameraUri = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        PhotoPicker.launch = { onResult ->
            pendingPhoto = onResult
            pickMedia.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
        }
        CameraCapture.launch = { onResult ->
            pendingCamera = onResult
            val dir = File(cacheDir, "captures").apply { mkdirs() }
            val file = File(dir, "capture_${System.currentTimeMillis()}.jpg")
            val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", file)
            pendingCameraUri = uri
            takePicture.launch(uri)
        }
        setContent { App() }
    }

    // Track the current Activity so window-bound capabilities (e.g. the confirm
    // dialog) can reach it — plugins only hold the Application context.
    override fun onResume() {
        super.onResume()
        MobilerActivity.current = WeakReference(this)
    }

    override fun onPause() {
        MobilerActivity.current = null
        super.onPause()
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
                // Cap + center the content column so a phone layout doesn't stretch
                // edge-to-edge on a tablet (the widthIn is a no-op on a phone).
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .verticalScroll(rememberScrollState()),
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .widthIn(max = 760.dp)
                            .align(Alignment.TopCenter)
                            .padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Render(view) { action -> core.update(action) }
                    }
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

        is Widget.ColorDot -> Box(
            modifier = Modifier.size(12.dp).clip(CircleShape).background(projectColorOf(widget.color)),
        )

        is Widget.Divider -> HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

        is Widget.Spacer -> Spacer(modifier = Modifier.height(spacingFor(widget.size)))

        is Widget.Row -> Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Let greedy inputs (which fill width) share the row with trailing
            // controls (buttons/chips/icons) instead of pushing them off-screen.
            widget.children.forEach { child ->
                when (child) {
                    is Widget.TextField, is Widget.Checkbox, is Widget.Column ->
                        Box(modifier = Modifier.weight(1f)) { Render(child, send) }
                    else -> Render(child, send)
                }
            }
        }

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

        is Widget.Grid -> BoxWithConstraints {
            // Column count follows the available width: 2 on a phone, more on a
            // tablet (the web/iOS twin of auto-fill / adaptive grids).
            val cols = maxOf(2, (maxWidth.value / 190f).toInt())
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                widget.children.chunked(cols).forEach { rowItems ->
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        rowItems.forEach { item -> Box(modifier = Modifier.weight(1f)) { Render(item, send) } }
                        repeat(cols - rowItems.size) { Spacer(modifier = Modifier.weight(1f)) }
                    }
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

        is Widget.Toggle -> Row(
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

        is Widget.Scaffold -> {
            // Hardware/gesture back pops the nav stack (fires the core's back
            // event). At the root (back == null) the system back exits the app.
            val back = widget.back
            if (back != null) {
                BackHandler { send(Action.Fired(back)) }
            }
            BoxWithConstraints {
                // On a wide screen (tablet / landscape) the bottom tab-bar becomes a
                // side navigation rail; a phone keeps its bottom tabs.
                val wide = maxWidth >= 600.dp && widget.tabs.isNotEmpty()
                Row(modifier = Modifier.fillMaxSize()) {
                    if (wide) {
                        NavigationRail {
                            widget.tabs.forEach { t ->
                                NavigationRailItem(
                                    selected = t.selected,
                                    onClick = { send(Action.Fired(t.onSelect)) },
                                    label = { Text(t.label) },
                                    icon = {},
                                )
                            }
                        }
                    }
                    Scaffold(
                        modifier = Modifier.fillMaxSize(),
                        topBar = {
                            CenterAlignedTopAppBar(
                                title = { Text(widget.title) },
                                navigationIcon = {
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
                            if (!wide && widget.tabs.isNotEmpty()) {
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
                        // Animate the body when the route changes: slide for push/pop
                        // (direction from depth), crossfade for a lateral move at the same
                        // depth. Same route = a data update → no transition (contentKey).
                        AnimatedContent(
                            targetState = widget,
                            contentKey = { it.route },
                            transitionSpec = {
                                val dur = 280
                                when {
                                    targetState.depth > initialState.depth ->
                                        (slideInHorizontally(tween(dur)) { it } + fadeIn(tween(dur))) togetherWith
                                            (slideOutHorizontally(tween(dur)) { -it / 3 } + fadeOut(tween(dur)))
                                    targetState.depth < initialState.depth ->
                                        (slideInHorizontally(tween(dur)) { -it / 3 } + fadeIn(tween(dur))) togetherWith
                                            (slideOutHorizontally(tween(dur)) { it } + fadeOut(tween(dur)))
                                    else ->
                                        fadeIn(tween(dur)) togetherWith fadeOut(tween(dur))
                                }
                            },
                            label = "nav",
                        ) { screen ->
                            // Cap + center the content column so it doesn't stretch on a tablet.
                            Box(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(padding)
                                    .verticalScroll(rememberScrollState()),
                            ) {
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .widthIn(max = 760.dp)
                                        .align(Alignment.TopCenter)
                                        .padding(horizontal = 16.dp),
                                    verticalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    Render(screen.body, send)
                                }
                            }
                        }
                    }
                }
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

// Concrete RGB for each project-identity color (same in light + dark — these are
// saturated identity colors, not theme surfaces).
private fun projectColorOf(color: ProjectColor): Color = when (color) {
    ProjectColor.INDIGO -> Color(0xFF5C6BC0)
    ProjectColor.TEAL -> Color(0xFF26A69A)
    ProjectColor.CORAL -> Color(0xFFFF7043)
    ProjectColor.AMBER -> Color(0xFFFFB300)
    ProjectColor.LIME -> Color(0xFF9CCC65)
    ProjectColor.PINK -> Color(0xFFEC407A)
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
