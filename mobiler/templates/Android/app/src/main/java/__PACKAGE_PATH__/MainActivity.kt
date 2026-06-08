package {{PACKAGE}}

import android.net.Uri
import android.os.Bundle
import androidx.core.content.FileProvider
import java.io.File
import java.lang.ref.WeakReference
import androidx.fragment.app.FragmentActivity
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
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.runtime.DisposableEffect
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CalendarMonth
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCut
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.FavoriteBorder
import androidx.compose.material.icons.filled.FilterList
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.People
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material.icons.filled.PhotoCamera
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.ShoppingCart
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.StarBorder
import androidx.compose.material.icons.filled.StarHalf
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
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
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.foundation.Canvas
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import kotlin.math.roundToInt
import androidx.lifecycle.viewmodel.compose.viewModel
import coil3.compose.AsyncImage
import {{PACKAGE}}.ui.theme.{{NAME}}Theme
import {{PACKAGE_SHARED_TYPES}}.Action
import {{PACKAGE_SHARED_TYPES}}.BoxAlign
import {{PACKAGE_SHARED_TYPES}}.ButtonStyle
import {{PACKAGE_SHARED_TYPES}}.Caption
import {{PACKAGE_SHARED_TYPES}}.CardStyle
import {{PACKAGE_SHARED_TYPES}}.ChartStyle
import {{PACKAGE_SHARED_TYPES}}.Corner
import {{PACKAGE_SHARED_TYPES}}.Density
import {{PACKAGE_SHARED_TYPES}}.FieldKind
import {{PACKAGE_SHARED_TYPES}}.Icon as WidgetIcon
import {{PACKAGE_SHARED_TYPES}}.ImageRatio
import {{PACKAGE_SHARED_TYPES}}.ImageShape
import {{PACKAGE_SHARED_TYPES}}.InputValue
import {{PACKAGE_SHARED_TYPES}}.ProjectColor
import {{PACKAGE_SHARED_TYPES}}.Spacing
import {{PACKAGE_SHARED_TYPES}}.TextStyle as ModelTextStyle
import {{PACKAGE_SHARED_TYPES}}.Theme as ModelTheme
import {{PACKAGE_SHARED_TYPES}}.Tone
import {{PACKAGE_SHARED_TYPES}}.Widget

// FragmentActivity (a ComponentActivity subclass — Compose/ActivityResult work unchanged) so the
// biometric plugin's androidx.biometric BiometricPrompt has the FragmentActivity host it requires.
class MainActivity : FragmentActivity() {
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
        // A deep link that launched the app — buffered by SystemBus until the core subscribes.
        intent?.data?.let { SystemBus.emitDeepLink(it.toString()) }
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

    // Inbound system events → SystemBus → the built-in `system` stream.
    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let { SystemBus.emitDeepLink(it.toString()) }
    }

    override fun onStart() {
        super.onStart()
        SystemBus.emitLifecycle("active")
    }

    override fun onStop() {
        SystemBus.emitLifecycle("background")
        super.onStop()
    }
}

@Composable
fun App(core: Core = viewModel()) {
    val view = core.view
    // Theme is data: a Scaffold carries dark_mode + an optional brand `theme`, from the Rust core.
    // Brand color, corner radius (Cards via MaterialTheme.shapes), and font flow through
    // MaterialTheme automatically. Density (spacing) + image-corner aren't MaterialTheme knobs,
    // so the non-composable mapper helpers (spacingFor/shapeFor) read them from `activeTheme`.
    val dark = (view as? Widget.Scaffold)?.darkMode ?: isSystemInDarkTheme()
    val appTheme = (view as? Widget.Scaffold)?.theme
    // Stash the active theme before rendering (app-global, like dark mode; render runs on the
    // main thread, so a plain holder is safe — the SwiftUI shell's `ActiveTheme` twin).
    activeTheme = appTheme
    {{NAME}}Theme(darkTheme = dark, theme = appTheme) {
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

        is Widget.PdfView -> PdfViewWidget(widget.url)
        is Widget.Video -> VideoWidget(widget.url, widget.id, widget.playing, widget.seekToMs, widget.controls, widget.looping, widget.muted, widget.onEnded, widget.poster, widget.startAtMs, widget.captions, widget.rate, widget.volume, widget.urls, widget.startIndex, widget.seekIndex, widget.allowPip, send)
        is Widget.WebView -> WebViewWidget(widget.url)

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

        is Widget.Avatar -> Box {
            AsyncImage(
                model = widget.source,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.size(48.dp).clip(CircleShape),
            )
            widget.status?.let { st ->
                Box(modifier = Modifier.size(12.dp).align(Alignment.BottomEnd).clip(CircleShape).background(toneColors(st).first))
            }
        }

        is Widget.Rating -> Row(verticalAlignment = Alignment.CenterVertically) {
            val tint = MaterialTheme.colorScheme.primary
            val onRate = widget.onRate
            for (i in 1..widget.max.toInt()) {
                val threshold = (i * 10).toUInt()
                val ic = when {
                    widget.value >= threshold -> Icons.Default.Star
                    widget.value + 5u >= threshold -> Icons.Default.StarHalf
                    else -> Icons.Default.StarBorder
                }
                if (onRate != null && i - 1 < onRate.size) {
                    val token = onRate[i - 1]
                    IconButton(onClick = { send(Action.Fired(token)) }, modifier = Modifier.size(32.dp)) {
                        Icon(ic, contentDescription = null, tint = tint)
                    }
                } else {
                    Icon(ic, contentDescription = null, tint = tint)
                }
            }
        }

        is Widget.Divider -> HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))
        is Widget.Progress -> {
            val v = widget.value
            if (v != null) {
                LinearProgressIndicator(progress = { v }, modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp))
            } else {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp))
            }
        }
        is Widget.Skeleton -> Box(
            modifier = Modifier.fillMaxWidth().height(48.dp).padding(vertical = 4.dp)
                .clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.onSurface.copy(alpha = 0.12f)),
        )
        is Widget.Chart -> {
            // Multi-series chart: cartesian (bar/line/stacked) with optional y-axis + legend, or
            // circular (pie/donut/rings/gauge). Series colors: explicit override → theme primary
            // (series 0) → a fixed palette. See mobiler-web's chart_view for the shared model.
            val series = widget.series
            val style = widget.style
            val primary = MaterialTheme.colorScheme.primary
            val trackColor = MaterialTheme.colorScheme.surfaceVariant
            val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
            val palette = listOf(
                Color(0xFFE0772C), Color(0xFF2EA06A), Color(0xFFC0466B),
                Color(0xFF8A5CC0), Color(0xFFC9A227), Color(0xFF3FA7D6),
            )
            fun colorFor(i: Int): Color {
                val c = series.getOrNull(i)?.color
                return when {
                    c != null -> Color(c.r.toInt(), c.g.toInt(), c.b.toInt())
                    i == 0 -> primary
                    else -> palette[(i - 1) % palette.size]
                }
            }
            fun mag(i: Int): Float = series.getOrNull(i)?.values?.sum() ?: 0f
            fun fmtTick(v: Float): String =
                if (kotlin.math.abs(v - kotlin.math.round(v)) < 0.05f) "${v.toInt()}" else "%.1f".format(v)
            val cartesian = style == ChartStyle.BAR || style == ChartStyle.LINE ||
                style == ChartStyle.STACKEDBAR || style == ChartStyle.STACKEDBAR100
            val nslots = (series.maxOfOrNull { it.values.size } ?: 0).coerceAtLeast(1)
            val maxV = when (style) {
                ChartStyle.STACKEDBAR -> (0 until nslots)
                    .map { j -> series.sumOf { (it.values.getOrNull(j) ?: 0f).toDouble() }.toFloat() }
                    .maxOrNull() ?: 1f
                ChartStyle.STACKEDBAR100 -> 1f
                else -> series.flatMap { it.values }.maxOrNull() ?: 1f
            }.coerceAtLeast(1e-6f)
            val capRound = androidx.compose.ui.graphics.StrokeCap.Round

            Column(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                if (cartesian) {
                    Row(modifier = Modifier.fillMaxWidth().height(120.dp)) {
                        if (widget.axis) {
                            Column(
                                modifier = Modifier.fillMaxHeight().padding(end = 4.dp),
                                verticalArrangement = Arrangement.SpaceBetween,
                            ) {
                                listOf(maxV, maxV / 2f, 0f).forEach {
                                    Text(fmtTick(it), style = MaterialTheme.typography.labelSmall, color = labelColor)
                                }
                            }
                        }
                        Canvas(modifier = Modifier.weight(1f).fillMaxHeight()) {
                            val w = size.width
                            val top = 2f
                            val plot = (size.height - 4f).coerceAtLeast(1f)
                            val bottom = top + plot
                            if (widget.axis) {
                                for (k in 0..4) {
                                    val y = top + k * plot / 4f
                                    drawLine(trackColor, Offset(0f, y), Offset(w, y), strokeWidth = 1f)
                                }
                            }
                            when (style) {
                                ChartStyle.LINE -> series.forEachIndexed { i, s ->
                                    val n = s.values.size.coerceAtLeast(1)
                                    val path = Path()
                                    s.values.forEachIndexed { j, v ->
                                        val x = if (n == 1) w / 2f else w * j / (n - 1)
                                        val y = top + (1f - (v / maxV).coerceIn(0f, 1f)) * plot
                                        if (j == 0) path.moveTo(x, y) else path.lineTo(x, y)
                                    }
                                    drawPath(path, colorFor(i), style = Stroke(width = 3f, cap = capRound))
                                }
                                ChartStyle.BAR -> {
                                    val sw = w / nslots
                                    val ns = series.size.coerceAtLeast(1)
                                    series.forEachIndexed { i, s ->
                                        s.values.forEachIndexed { j, v ->
                                            val bh = (v / maxV).coerceIn(0f, 1f) * plot
                                            val bw = sw * 0.8f / ns
                                            val x = j * sw + sw * 0.1f + i * bw
                                            drawRect(colorFor(i), topLeft = Offset(x, bottom - bh), size = Size(bw, bh))
                                        }
                                    }
                                }
                                else -> {
                                    val sw = w / nslots
                                    for (j in 0 until nslots) {
                                        val slotTotal = series
                                            .sumOf { (it.values.getOrNull(j) ?: 0f).toDouble() }
                                            .toFloat().coerceAtLeast(1e-6f)
                                        val denom = if (style == ChartStyle.STACKEDBAR100) slotTotal else maxV
                                        var acc = 0f
                                        series.forEachIndexed { i, s ->
                                            val v = s.values.getOrNull(j) ?: 0f
                                            val bh = (v / denom).coerceIn(0f, 1f) * plot
                                            val x = j * sw + sw * 0.15f
                                            drawRect(colorFor(i), topLeft = Offset(x, bottom - acc - bh), size = Size(sw * 0.7f, bh))
                                            acc += bh
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if (widget.labels.isNotEmpty()) {
                        Row(modifier = Modifier.fillMaxWidth()) {
                            widget.labels.forEach {
                                Text(it, style = MaterialTheme.typography.labelSmall, color = labelColor, modifier = Modifier.weight(1f), textAlign = TextAlign.Center)
                            }
                        }
                    }
                } else {
                    val gaugePct = if (style == ChartStyle.GAUGE) {
                        val g = series.firstOrNull()?.goal ?: mag(0)
                        ((mag(0) / (if (g <= 0f) 1e-6f else g)).coerceIn(0f, 1f) * 100f).toInt()
                    } else {
                        0
                    }
                    Box(modifier = Modifier.fillMaxWidth().height(140.dp), contentAlignment = Alignment.Center) {
                        Canvas(modifier = Modifier.fillMaxWidth().fillMaxHeight()) {
                            val cx = size.width / 2f
                            val cy = size.height / 2f
                            val rad = (minOf(size.width, size.height) / 2f) - 6f
                            when (style) {
                                ChartStyle.PIE, ChartStyle.DONUT -> {
                                    val total = (0 until series.size).sumOf { mag(it).toDouble() }.toFloat().coerceAtLeast(1e-6f)
                                    var start = -90f
                                    series.forEachIndexed { i, _ ->
                                        val sweep = mag(i) / total * 360f
                                        if (style == ChartStyle.DONUT) {
                                            val r = rad - 10f
                                            drawArc(colorFor(i), start, sweep, useCenter = false, topLeft = Offset(cx - r, cy - r), size = Size(r * 2f, r * 2f), style = Stroke(width = 20f))
                                        } else {
                                            drawArc(colorFor(i), start, sweep, useCenter = true, topLeft = Offset(cx - rad, cy - rad), size = Size(rad * 2f, rad * 2f))
                                        }
                                        start += sweep
                                    }
                                }
                                ChartStyle.RINGS -> {
                                    val n = series.size.coerceAtLeast(1)
                                    series.forEachIndexed { i, s ->
                                        val r = rad - i * (rad * 0.62f / n) - 2f
                                        val tl = Offset(cx - r, cy - r)
                                        val sz = Size(r * 2f, r * 2f)
                                        val g = s.goal ?: mag(i)
                                        val prog = (mag(i) / (if (g <= 0f) 1e-6f else g)).coerceIn(0f, 1f)
                                        drawArc(trackColor, 0f, 360f, useCenter = false, topLeft = tl, size = sz, style = Stroke(width = 10f))
                                        drawArc(colorFor(i), -90f, 360f * prog, useCenter = false, topLeft = tl, size = sz, style = Stroke(width = 10f, cap = capRound))
                                    }
                                }
                                else -> {
                                    val r = rad - 4f
                                    val tl = Offset(cx - r, cy - r)
                                    val sz = Size(r * 2f, r * 2f)
                                    val g = series.firstOrNull()?.goal ?: mag(0)
                                    val prog = (mag(0) / (if (g <= 0f) 1e-6f else g)).coerceIn(0f, 1f)
                                    drawArc(trackColor, 135f, 270f, useCenter = false, topLeft = tl, size = sz, style = Stroke(width = 12f, cap = capRound))
                                    drawArc(colorFor(0), 135f, 270f * prog, useCenter = false, topLeft = tl, size = sz, style = Stroke(width = 12f, cap = capRound))
                                }
                            }
                        }
                        if (style == ChartStyle.GAUGE) {
                            Text("$gaugePct%", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.onSurface)
                        }
                    }
                }
                if (widget.legend && series.isNotEmpty()) {
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally),
                    ) {
                        series.forEachIndexed { i, s ->
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                Box(modifier = Modifier.size(10.dp).clip(RoundedCornerShape(2.dp)).background(colorFor(i)))
                                Text(s.name, style = MaterialTheme.typography.labelSmall, color = labelColor)
                            }
                        }
                    }
                }
            }
        }

        is Widget.RegionChart -> {
            // Variable-width stacked-region ("coverage-gap") chart: absolute-positioned region
            // rectangles in the [0,xMax]×[0,yMax] plane + ref lines/chips + irregular x-ticks +
            // optional right bracket + legend. The Android twin of mobiler-web's region_chart_view.
            val xm = widget.xMax.coerceAtLeast(1e-6f)
            val ym = widget.yMax.coerceAtLeast(1e-6f)
            val palette = listOf(
                Color(0xFFE0772C), Color(0xFF2EA06A), Color(0xFFC0466B),
                Color(0xFF8A5CC0), Color(0xFFC9A227), Color(0xFF3FA7D6),
            )
            fun regionColor(i: Int): Color {
                val c = widget.regions.getOrNull(i)?.color
                return if (c != null) Color(c.r.toInt(), c.g.toInt(), c.b.toInt()) else palette[i % palette.size]
            }
            // Black or white label text, whichever reads on the band's fill (perceived luminance).
            fun textOn(c: Color): Color =
                if (0.299f * c.red + 0.587f * c.green + 0.114f * c.blue > 0.55f) Color(0xFF1A1A1A) else Color(0xFFF5F5F5)
            fun fmtTick(v: Float): String =
                if (kotlin.math.abs(v - kotlin.math.round(v)) < 0.05f) "${v.toInt()}" else "%.1f".format(v)
            val labelColor = MaterialTheme.colorScheme.onSurfaceVariant
            val refColor = Color(0xFFC0392B)
            val axisColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.8f)
            val bracketColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.55f)
            Column(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                Row(modifier = Modifier.fillMaxWidth()) {
                    Column(
                        modifier = Modifier.width(40.dp).height(300.dp).padding(end = 3.dp),
                        verticalArrangement = Arrangement.SpaceBetween,
                        horizontalAlignment = Alignment.End,
                    ) {
                        for (k in 4 downTo 0) {
                            Text(fmtTick(ym * k / 4f), style = MaterialTheme.typography.labelSmall, fontSize = 9.sp, color = labelColor, maxLines = 1)
                        }
                    }
                    Column(modifier = Modifier.weight(1f)) {
                        BoxWithConstraints(modifier = Modifier.fillMaxWidth().height(300.dp)) {
                            val pw = maxWidth
                            val ph = maxHeight
                            widget.regions.forEachIndexed { i, r ->
                                Box(
                                    modifier = Modifier
                                        .offset(x = pw * (r.x0 / xm), y = ph * (1f - r.y1 / ym))
                                        .width(pw * ((r.x1 - r.x0) / xm))
                                        .height(ph * ((r.y1 - r.y0) / ym))
                                        .background(regionColor(i))
                                        .border(0.5.dp, Color.White.copy(alpha = 0.4f)),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    if (r.label.isNotEmpty()) {
                                        Text(
                                            r.label,
                                            style = MaterialTheme.typography.labelSmall,
                                            color = textOn(regionColor(i)),
                                            textAlign = TextAlign.Center,
                                            maxLines = 3,
                                            modifier = if (r.vertical) Modifier.rotate(-90f) else Modifier,
                                        )
                                    }
                                }
                            }
                            Canvas(modifier = Modifier.fillMaxSize()) {
                                widget.refLines.forEach { rl ->
                                    val y = size.height * (1f - rl.value / ym)
                                    val effect = if (rl.dashed) PathEffect.dashPathEffect(floatArrayOf(10f, 8f)) else null
                                    drawLine(refColor, Offset(0f, y), Offset(size.width, y), strokeWidth = 2f, pathEffect = effect)
                                }
                                widget.bracket?.let { b ->
                                    val yt = size.height * (1f - b.y1 / ym)
                                    val yb = size.height * (1f - b.y0 / ym)
                                    val x = size.width - 2f
                                    drawLine(bracketColor, Offset(x, yt), Offset(x, yb), strokeWidth = 2f)
                                    drawLine(bracketColor, Offset(x, yt), Offset(x - 6f, yt), strokeWidth = 2f)
                                    drawLine(bracketColor, Offset(x, yb), Offset(x - 6f, yb), strokeWidth = 2f)
                                }
                                drawLine(axisColor, Offset(0f, 0f), Offset(0f, size.height), strokeWidth = 2f)
                                drawLine(axisColor, Offset(0f, size.height), Offset(size.width, size.height), strokeWidth = 2f)
                                for (k in 0..4) {
                                    val y = size.height * k / 4f
                                    drawLine(axisColor, Offset(0f, y), Offset(6f, y), strokeWidth = 1.5f)
                                }
                                widget.ticks.forEach { t ->
                                    val tx = size.width * (t.at / xm)
                                    drawLine(axisColor, Offset(tx, size.height), Offset(tx, size.height - 6f), strokeWidth = 1.5f)
                                }
                            }
                            widget.refLines.forEach { rl ->
                                val y = ph * (1f - rl.value / ym)
                                Box(modifier = Modifier.offset(x = pw - 78.dp, y = y + 2.dp)) {
                                    Text(
                                        rl.label,
                                        style = MaterialTheme.typography.labelSmall,
                                        fontSize = 9.sp,
                                        color = Color(0xFF1A1A1A),
                                        modifier = Modifier
                                            .background(Color.White, RoundedCornerShape(4.dp))
                                            .border(0.5.dp, Color(0x33000000), RoundedCornerShape(4.dp))
                                            .padding(horizontal = 4.dp, vertical = 1.dp),
                                    )
                                }
                            }
                            widget.bracket?.let { b ->
                                val yc = ph * (1f - (b.y0 + b.y1) / 2f / ym)
                                Box(modifier = Modifier.offset(x = pw - 70.dp, y = yc - 14.dp).width(64.dp)) {
                                    Text(
                                        (if (b.info) "ⓘ\n" else "") + b.label,
                                        style = MaterialTheme.typography.labelSmall,
                                        fontSize = 8.sp,
                                        color = labelColor,
                                        textAlign = TextAlign.Center,
                                        maxLines = 4,
                                    )
                                }
                            }
                        }
                        BoxWithConstraints(modifier = Modifier.fillMaxWidth().height(16.dp)) {
                            val pw = maxWidth
                            widget.ticks.forEach { t ->
                                Text(
                                    t.label,
                                    style = MaterialTheme.typography.labelSmall,
                                    color = labelColor,
                                    maxLines = 1,
                                    modifier = Modifier.offset(x = (pw * (t.at / xm)) - 24.dp),
                                )
                            }
                        }
                    }
                }
                if (widget.legend.isNotEmpty()) {
                    Column(modifier = Modifier.fillMaxWidth().padding(top = 6.dp)) {
                        widget.legend.chunked(3).forEach { rowItems ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally),
                            ) {
                                rowItems.forEach { li ->
                                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                                        Box(modifier = Modifier.size(10.dp).clip(RoundedCornerShape(2.dp)).background(Color(li.color.r.toInt(), li.color.g.toInt(), li.color.b.toInt())))
                                        Text(li.label, style = MaterialTheme.typography.labelSmall, color = labelColor)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        is Widget.Calendar -> {
            val weekdays = listOf("S", "M", "T", "W", "T", "F", "S")
            val months = listOf("January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December")
            val cells = ArrayList<Int?>()
            repeat(widget.firstWeekday.toInt()) { cells.add(null) }
            for (d in 1..widget.onDay.size) cells.add(d)
            Column(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                Text("${months[widget.month.toInt() - 1]} ${widget.year}", style = MaterialTheme.typography.titleMedium)
                Row(modifier = Modifier.fillMaxWidth()) {
                    weekdays.forEach { Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f), textAlign = TextAlign.Center) }
                }
                cells.chunked(7).forEach { week ->
                    Row(modifier = Modifier.fillMaxWidth()) {
                        week.forEach { day ->
                            if (day == null) {
                                Box(modifier = Modifier.weight(1f).height(40.dp))
                            } else {
                                val isSel = widget.selected?.toInt() == day
                                val token = widget.onDay[day - 1]
                                Box(
                                    modifier = Modifier.weight(1f).height(40.dp).padding(2.dp)
                                        .clip(CircleShape)
                                        .background(if (isSel) MaterialTheme.colorScheme.primary else Color.Transparent)
                                        .clickable { send(Action.Fired(token)) },
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text("$day", color = if (isSel) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface)
                                }
                            }
                        }
                        repeat(7 - week.size) { Box(modifier = Modifier.weight(1f)) }
                    }
                }
            }
        }
        is Widget.SwipeAction -> {
            val actions = widget.actions
            val density = LocalDensity.current
            val revealPx = with(density) { (actions.size * 84).dp.toPx() }
            var offsetX by remember(widget) { mutableStateOf(0f) }
            Box(modifier = Modifier.fillMaxWidth()) {
                Row(modifier = Modifier.matchParentSize(), horizontalArrangement = Arrangement.End) {
                    actions.forEach { a ->
                        val (_, fg) = toneColors(a.tone)
                        Box(
                            modifier = Modifier.fillMaxHeight().width(84.dp).padding(vertical = 4.dp, horizontal = 4.dp).clip(RoundedCornerShape(12.dp)).background(fg)
                                .clickable { send(Action.Fired(a.onTap)); offsetX = 0f },
                            contentAlignment = Alignment.Center,
                        ) { Text(a.label, color = Color.White, style = MaterialTheme.typography.labelMedium) }
                    }
                }
                Box(
                    modifier = Modifier.fillMaxWidth()
                        .offset { IntOffset(offsetX.roundToInt(), 0) }
                        .background(MaterialTheme.colorScheme.surface)
                        .draggable(
                            orientation = Orientation.Horizontal,
                            state = rememberDraggableState { delta -> offsetX = (offsetX + delta).coerceIn(-revealPx, 0f) },
                        ),
                ) { Render(widget.child, send) }
            }
        }
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
                CardStyle.BRAND -> {
                    // Brand gradient (seed → accent, via the M3 primary → secondary scheme).
                    val cs = MaterialTheme.colorScheme
                    val grad = Brush.linearGradient(listOf(cs.primary, cs.secondary))
                    val brandMod = mod
                        .clip(MaterialTheme.shapes.medium)
                        .background(grad)
                        .then(if (op != null) Modifier.clickable { send(Action.Fired(op)) } else Modifier)
                    Box(modifier = brandMod) {
                        CompositionLocalProvider(LocalContentColor provides Color.White) { CardBody(widget.child, send) }
                    }
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

        is Widget.Scroller -> Row(
            modifier = Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) { widget.children.forEach { Render(it, send) } }

        // A paged feed list: pull-to-refresh on top (PullToRefreshBox) + load-more when the user
        // scrolls near the end. Bounded height so the inner LazyColumn scrolls inside the body.
        is Widget.LazyList -> {
            val onLoadMore = widget.onLoadMore
            val loading = widget.loading
            val hasMore = widget.hasMore
            val list: @Composable () -> Unit = {
                LazyColumn(
                    modifier = Modifier.fillMaxWidth().heightIn(max = 480.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    itemsIndexed(widget.children) { idx, child ->
                        Render(child, send)
                        // Fire load-more once when the second-to-last item composes (near the end).
                        if (onLoadMore != null && hasMore && !loading && idx >= widget.children.size - 2) {
                            LaunchedEffect(widget.children.size, idx) { send(Action.Fired(onLoadMore)) }
                        }
                    }
                    if (loading) {
                        item { LinearProgressIndicator(modifier = Modifier.fillMaxWidth().padding(8.dp)) }
                    } else if (!hasMore && onLoadMore != null) {
                        item {
                            Text(
                                "End of list",
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                textAlign = TextAlign.Center,
                                modifier = Modifier.fillMaxWidth().padding(8.dp),
                            )
                        }
                    }
                }
            }
            val onRefresh = widget.onRefresh
            if (onRefresh != null) {
                PullToRefreshBox(
                    isRefreshing = widget.refreshing,
                    onRefresh = { send(Action.Fired(onRefresh)) },
                    modifier = Modifier.fillMaxWidth().heightIn(max = 480.dp),
                ) { list() }
            } else {
                list()
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

        is Widget.TextField -> {
            val multiline = widget.kind == FieldKind.MULTILINE
            val kbType = when (widget.kind) {
                FieldKind.EMAIL -> KeyboardType.Email
                FieldKind.NUMBER -> KeyboardType.Number
                FieldKind.DECIMAL -> KeyboardType.Decimal
                FieldKind.PHONE -> KeyboardType.Phone
                FieldKind.URL -> KeyboardType.Uri
                FieldKind.SECURE -> KeyboardType.Password
                else -> KeyboardType.Text
            }
            OutlinedTextField(
                value = widget.value,
                onValueChange = { send(Action.Input(widget.id, InputValue.Text(it))) },
                placeholder = { Text(widget.placeholder) },
                singleLine = !multiline,
                minLines = if (multiline) 3 else 1,
                visualTransformation = if (widget.kind == FieldKind.SECURE) PasswordVisualTransformation() else VisualTransformation.None,
                keyboardOptions = KeyboardOptions(keyboardType = kbType),
                isError = widget.error != null,
                supportingText = widget.error?.let { msg -> { Text(msg) } },
                modifier = Modifier.fillMaxWidth(),
            )
        }

        is Widget.SearchField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { send(Action.Input(widget.id, InputValue.Text(it))) },
            placeholder = { Text(widget.placeholder) },
            leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
            singleLine = true,
            shape = RoundedCornerShape(50),
            modifier = Modifier.fillMaxWidth(),
        )

        is Widget.Segmented -> SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
            widget.segments.forEachIndexed { i, seg ->
                SegmentedButton(
                    selected = seg.selected,
                    onClick = { send(Action.Fired(seg.onSelect)) },
                    shape = SegmentedButtonDefaults.itemShape(index = i, count = widget.segments.size),
                ) { Text(seg.label) }
            }
        }

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
            // Modal bottom sheet — present in the tree ⇒ shown (a Popup, so it overlays
            // everything regardless of where it's composed). Scrim/swipe fires on_dismiss.
            widget.sheet?.let { sheet ->
                ModalBottomSheet(onDismissRequest = { send(Action.Fired(sheet.onDismiss)) }) {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(sheet.title, style = MaterialTheme.typography.titleLarge)
                        Render(sheet.child, send)
                    }
                }
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
                                    icon = { t.icon?.let { Icon(iconFor(it), contentDescription = null) } },
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
                                            icon = { t.icon?.let { Icon(iconFor(it), contentDescription = null) } },
                                        )
                                    }
                                }
                            }
                        },
                        floatingActionButton = {
                            widget.fab?.let { fab ->
                                FloatingActionButton(onClick = { send(Action.Fired(fab.onPress)) }) {
                                    Icon(iconFor(fab.icon), contentDescription = null)
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
                            val column: @Composable BoxScope.() -> Unit = {
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .widthIn(max = 760.dp)
                                        .align(Alignment.TopCenter)
                                        .padding(horizontal = 16.dp),
                                    verticalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    if (screen.refreshing) {
                                        LinearProgressIndicator(modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp))
                                    }
                                    Render(screen.body, send)
                                }
                            }
                            val onRefresh = screen.onRefresh
                            if (onRefresh != null) {
                                // Pull-to-refresh — the body is pull-refreshable; the spinner is
                                // driven by the app-owned `refreshing` flag.
                                PullToRefreshBox(
                                    isRefreshing = screen.refreshing,
                                    onRefresh = { send(Action.Fired(onRefresh)) },
                                    modifier = Modifier.fillMaxSize().padding(padding),
                                ) {
                                    Box(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()), content = column)
                                }
                            } else {
                                Box(
                                    modifier = Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()),
                                    content = column,
                                )
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

// Active app theme (set by App() when a Scaffold renders), read by the non-composable mapper
// helpers below for density (spacing) + image-corner — knobs MaterialTheme can't carry.
// null = framework defaults (no visual change). App-global, like dark mode; the SwiftUI
// shell's `ActiveTheme.current` twin.
private var activeTheme: ModelTheme? = null

// Spacing multiplier from the theme's density. Comfortable (or un-themed) = 1.0; Compact tightens.
private val densityScale: Float
    get() = when (activeTheme?.density) {
        Density.COMPACT -> 0.75f
        Density.COMFORTABLE, null -> 1.0f
    }

// Image corner radius (dp) from the theme's corner; 16 when un-themed (the original look).
private val imageCornerDp: Int
    get() = when (activeTheme?.corner) {
        Corner.NONE -> 0
        Corner.SMALL -> 10
        Corner.MEDIUM -> 16
        Corner.LARGE -> 24
        null -> 16
    }

private fun spacingFor(size: Spacing): Dp {
    val base = when (size) {
        Spacing.XS -> 4
        Spacing.SM -> 8
        Spacing.MD -> 12
        Spacing.LG -> 16
        Spacing.XL -> 24
    }
    return (base * densityScale).dp
}

private fun iconFor(icon: WidgetIcon): androidx.compose.ui.graphics.vector.ImageVector = when (icon) {
    WidgetIcon.DELETE -> Icons.Default.Delete
    WidgetIcon.ADD -> Icons.Default.Add
    WidgetIcon.EDIT -> Icons.Default.Edit
    WidgetIcon.CLOSE -> Icons.Default.Close
    WidgetIcon.SETTINGS -> Icons.Default.Settings
    WidgetIcon.CHECK -> Icons.Default.Check
    WidgetIcon.STAR -> Icons.Default.Star
    WidgetIcon.INFO -> Icons.Default.Info
    WidgetIcon.HOME -> Icons.Default.Home
    WidgetIcon.SEARCH -> Icons.Default.Search
    WidgetIcon.MENU -> Icons.Default.Menu
    WidgetIcon.FILTER -> Icons.Default.FilterList
    WidgetIcon.BACK -> Icons.AutoMirrored.Filled.ArrowBack
    WidgetIcon.FORWARD -> Icons.AutoMirrored.Filled.ArrowForward
    WidgetIcon.DOWN -> Icons.Default.KeyboardArrowDown
    WidgetIcon.BELL -> Icons.Default.Notifications
    WidgetIcon.CART -> Icons.Default.ShoppingCart
    WidgetIcon.SHARE -> Icons.Default.Share
    WidgetIcon.HEART -> Icons.Default.FavoriteBorder
    WidgetIcon.HEARTFILLED -> Icons.Default.Favorite
    WidgetIcon.PERSON -> Icons.Default.Person
    WidgetIcon.PEOPLE -> Icons.Default.People
    WidgetIcon.PHONE -> Icons.Default.Phone
    WidgetIcon.MAIL -> Icons.Default.Email
    WidgetIcon.CALENDAR -> Icons.Default.CalendarMonth
    WidgetIcon.CLOCK -> Icons.Default.Schedule
    WidgetIcon.MAPPIN -> Icons.Default.Place
    WidgetIcon.CAMERA -> Icons.Default.PhotoCamera
    WidgetIcon.PHOTO -> Icons.Default.Image
    WidgetIcon.PLAY -> Icons.Default.PlayArrow
    WidgetIcon.SCISSORS -> Icons.Default.ContentCut
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

// Native video player (Widget.Video) — Media3 ExoPlayer in a PlayerView. The app drives play/pause
// (`playing`) + seek (`seekToMs`, applied when it changes); a 1s loop reports the position via
// Action.Input(id, Int(ms)); onEnded fires (loop is handled by repeatMode). The player is keyed by
// `url` (remember) and released on dispose — never leaked across the ~1/sec re-render.
// Build a MediaItem with optional sidecar WebVTT caption tracks (Media3 renders them natively).
@OptIn(androidx.media3.common.util.UnstableApi::class)
private fun buildVideoItem(url: String, captions: List<Caption>): MediaItem {
    val b = MediaItem.Builder().setUri(url)
    if (captions.isNotEmpty()) {
        b.setSubtitleConfigurations(captions.map { c ->
            MediaItem.SubtitleConfiguration.Builder(android.net.Uri.parse(c.url))
                .setMimeType(androidx.media3.common.MimeTypes.TEXT_VTT)
                .setLanguage(c.language)
                .setLabel(c.label)
                .setSelectionFlags(if (c.defaultOn) androidx.media3.common.C.SELECTION_FLAG_DEFAULT else 0)
                .build()
        })
    }
    return b.build()
}

@OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun VideoWidget(url: String, id: String, playing: Boolean, seekToMs: Long, controls: Boolean, looping: Boolean, muted: Boolean, onEnded: String?, poster: String?, startAtMs: Long, captions: List<Caption>, rate: Float, volume: Float, urls: List<String>, startIndex: Long, seekIndex: Long, allowPip: Boolean, send: (Action) -> Unit) {
    val context = LocalContext.current
    val player = remember(url, urls) {
        ExoPlayer.Builder(context).build().apply {
            if (urls.isEmpty()) {
                setMediaItem(buildVideoItem(url, captions))
            } else {
                setMediaItems(urls.map { buildVideoItem(it, emptyList()) }, startIndex.toInt().coerceIn(0, maxOf(0, urls.size - 1)), 0L)
            }
            prepare()
        }
    }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                if (state == Player.STATE_ENDED && onEnded != null) send(Action.Fired(onEnded))
            }
        }
        player.addListener(listener)
        onDispose { player.removeListener(listener); player.release() }
    }
    LaunchedEffect(looping) { player.repeatMode = if (looping) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF }
    LaunchedEffect(muted, volume) { player.volume = if (muted) 0f else volume.coerceIn(0f, 1f) }
    LaunchedEffect(rate) { player.setPlaybackSpeed(if (rate > 0f) rate else 1f) }
    // PiP: on Android 12+ auto-enter when the app is backgrounded (no button needed). Gated by allowPip;
    // needs android:supportsPictureInPicture on the activity (template manifest).
    val activity = context as? android.app.Activity
    LaunchedEffect(allowPip) {
        if (allowPip && activity != null && android.os.Build.VERSION.SDK_INT >= 31) {
            activity.setPictureInPictureParams(android.app.PictureInPictureParams.Builder().setAutoEnterEnabled(true).build())
        }
    }
    // Position + transport state (duration / state / buffered / playlist index) via suffixed input ids.
    var lastDur by remember { mutableStateOf(Long.MIN_VALUE) }
    var lastState by remember { mutableStateOf(-1L) }
    var lastIndex by remember { mutableStateOf(-2L) }
    LaunchedEffect(player, id) {
        while (true) {
            kotlinx.coroutines.delay(1000)
            send(Action.Input(id, InputValue.Int(player.currentPosition)))
            val dur = if (player.duration == androidx.media3.common.C.TIME_UNSET) -1L else player.duration
            if (dur != lastDur) { lastDur = dur; send(Action.Input("$id.duration", InputValue.Int(dur))) }
            send(Action.Input("$id.buffered", InputValue.Int(player.bufferedPosition)))
            val state = when {
                player.playbackState == Player.STATE_ENDED -> 4L
                player.playbackState == Player.STATE_BUFFERING -> 1L
                player.isPlaying -> 3L
                player.playbackState == Player.STATE_READY -> 2L
                else -> 0L
            }
            if (state != lastState) { lastState = state; send(Action.Input("$id.state", InputValue.Int(state))) }
            val idx = player.currentMediaItemIndex.toLong()
            if (idx != lastIndex) { lastIndex = idx; send(Action.Input("$id.index", InputValue.Int(idx))) }
        }
    }
    LaunchedEffect(playing) { player.playWhenReady = playing }
    var lastSeek by remember { mutableStateOf(-1L) }
    LaunchedEffect(seekToMs) {
        if (seekToMs >= 0 && seekToMs != lastSeek) { lastSeek = seekToMs; player.seekTo(seekToMs) }
        else if (seekToMs < 0) { lastSeek = -1L }
    }
    var startApplied by remember { mutableStateOf(false) }
    LaunchedEffect(player) {
        if (startAtMs >= 0 && !startApplied) { startApplied = true; player.seekTo(startAtMs) }
    }
    var lastSeekIndex by remember { mutableStateOf(-1L) }
    LaunchedEffect(seekIndex) {
        if (seekIndex >= 0 && seekIndex != lastSeekIndex) { lastSeekIndex = seekIndex; player.seekTo(seekIndex.toInt(), 0L) }
        else if (seekIndex < 0) { lastSeekIndex = -1L }
    }
    // Poster: fetch the image off the main thread, show it as PlayerView artwork while idle.
    val posterDrawable = remember(poster) { mutableStateOf<android.graphics.drawable.Drawable?>(null) }
    LaunchedEffect(poster) {
        if (poster != null) {
            posterDrawable.value = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
                try {
                    java.net.URL(poster).openStream().use {
                        android.graphics.drawable.BitmapDrawable(context.resources, android.graphics.BitmapFactory.decodeStream(it))
                    }
                } catch (e: Exception) { null }
            }
        }
    }
    AndroidView(
        factory = { ctx -> PlayerView(ctx).apply { this.player = player; useController = controls } },
        update = { view ->
            view.useController = controls
            view.defaultArtwork = posterDrawable.value
            view.artworkDisplayMode = if (posterDrawable.value != null) PlayerView.ARTWORK_DISPLAY_MODE_FILL else PlayerView.ARTWORK_DISPLAY_MODE_OFF
        },
        modifier = Modifier.fillMaxWidth().heightIn(min = 240.dp),
    )
}

// General embedded web content (Widget.WebView) — an android.webkit.WebView in an AndroidView. JS +
// inline-media autoplay are enabled so hosted players (e.g. Bunny.net embeds) work. Keyed by `url`
// (remember) and destroyed on dispose — never leaked.
@Composable
private fun WebViewWidget(url: String) {
    val context = LocalContext.current
    val webView = remember(url) {
        android.webkit.WebView(context).apply {
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.mediaPlaybackRequiresUserGesture = false
            webViewClient = android.webkit.WebViewClient()
            loadUrl(url)
        }
    }
    DisposableEffect(webView) { onDispose { webView.destroy() } }
    AndroidView(
        factory = { webView },
        modifier = Modifier.fillMaxWidth().heightIn(min = 240.dp),
    )
}

/** In-app PDF viewer (Widget.PdfView). Downloads a remote PDF to the cache (or opens a file URI),
 *  renders every page to a bitmap with the platform PdfRenderer, and stacks them. The parent
 *  scroller handles overflow. */
@Composable
private fun PdfViewWidget(url: String) {
    val context = LocalContext.current
    var pages by remember(url) { mutableStateOf<List<ImageBitmap>>(emptyList()) }
    var error by remember(url) { mutableStateOf<String?>(null) }
    LaunchedEffect(url) {
        try {
            pages = withContext(Dispatchers.IO) { renderPdfPages(context, url) }
        } catch (e: Exception) {
            error = e.message ?: "could not load PDF"
        }
    }
    Column(modifier = Modifier.fillMaxWidth().heightIn(min = 480.dp)) {
        when {
            error != null -> Text("PDF: $error")
            pages.isEmpty() -> CircularProgressIndicator(modifier = Modifier.padding(16.dp))
            else -> pages.forEach { page ->
                Image(
                    bitmap = page,
                    contentDescription = null,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                )
            }
        }
    }
}

private fun renderPdfPages(context: android.content.Context, url: String): List<ImageBitmap> {
    val file: java.io.File = if (url.startsWith("http")) {
        val tmp = java.io.File.createTempFile("mobiler_pdf", ".pdf", context.cacheDir)
        java.net.URL(url).openStream().use { input -> tmp.outputStream().use { output -> input.copyTo(output) } }
        tmp
    } else {
        val uri = android.net.Uri.parse(url)
        if (uri.scheme == "file") java.io.File(uri.path!!) else java.io.File(url)
    }
    val pfd = android.os.ParcelFileDescriptor.open(file, android.os.ParcelFileDescriptor.MODE_READ_ONLY)
    val renderer = android.graphics.pdf.PdfRenderer(pfd)
    val pages = ArrayList<ImageBitmap>(renderer.pageCount)
    for (i in 0 until renderer.pageCount) {
        val page = renderer.openPage(i)
        val width = 1080
        val height = (width.toFloat() / page.width * page.height).toInt().coerceAtLeast(1)
        val bmp = android.graphics.Bitmap.createBitmap(width, height, android.graphics.Bitmap.Config.ARGB_8888)
        bmp.eraseColor(android.graphics.Color.WHITE)
        page.render(bmp, null, null, android.graphics.pdf.PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
        page.close()
        pages.add(bmp.asImageBitmap())
    }
    renderer.close()
    pfd.close()
    return pages
}

private fun shapeFor(shape: ImageShape): Shape = when (shape) {
    ImageShape.SQUARE -> RectangleShape
    ImageShape.ROUNDED -> RoundedCornerShape(imageCornerDp.dp)
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
