#include "shim.h"
#include <chrono>
#include "iconprovider.h"
#include "layersurface.h"
#include "style.h"
#include "kynex-shell/src/clock.cxxqt.h"
#include "kynex-shell/src/launcher.cxxqt.h"
#include "kynex-shell/src/network.cxxqt.h"
#include "kynex-shell/src/notifications.cxxqt.h"
#include "kynex-shell/src/power.cxxqt.h"
#include "kynex-shell/src/windows.cxxqt.h"
#include <QtQml>
#include <QFontDatabase>
#include <QDir>
#include <QCoreApplication>

void kynex_register_core_types()
{
    qmlRegisterType<LayerSurface>("Kynex.Shell", 1, 0, "LayerSurface");
    qmlRegisterType<Clock>("Kynex.Shell", 1, 0, "Clock");
    qmlRegisterType<LauncherModel>("Kynex.Shell", 1, 0, "LauncherModel");
    qmlRegisterType<PowerModel>("Kynex.Shell", 1, 0, "PowerModel");
    qmlRegisterType<NetworkModel>("Kynex.Shell", 1, 0, "NetworkModel");
    qmlRegisterType<NotificationModel>("Kynex.Shell", 1, 0, "NotificationModel");
    qmlRegisterType<WindowListModel>("Kynex.Shell", 1, 0, "WindowListModel");
    qmlRegisterType<WindowListModel>("Kynex.Shell", 1, 0, "TaskbarModel");
    qmlRegisterSingletonInstance<Style>("Kynex.Style", 1, 0, "Style", Style::instance());
}

void kynex_register_image_provider(QQmlApplicationEngine &engine)
{
    kynex_register_image_provider_app(engine);
}

bool kynex_is_svg_safe_slice(rust::Slice<const uint8_t> data)
{
    QByteArray bytes(reinterpret_cast<const char *>(data.data()), static_cast<qsizetype>(data.size()));
    return kynex_is_svg_safe(bytes);
}

bool kynex_test_decode_icon(rust::Str path, int32_t width, int32_t height, int32_t scale)
{
    QString qpath = QString::fromUtf8(path.data(), static_cast<qsizetype>(path.size()));
    QSize size(width, height);
    QImage img = kynex_decode_icon(qpath, size, scale);
    return !img.isNull();
}

void kynex_load_bundled_fonts()
{
    QStringList fontDirs = {
        QStringLiteral("assets/fonts"),
        QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("../assets/fonts")),
        QStringLiteral("/usr/share/kynex/fonts"),
        QStringLiteral("/usr/local/share/kynex/fonts")
    };

    QByteArray envDir = qgetenv("KYNEX_FONTS_DIR");
    if (!envDir.isEmpty()) {
        fontDirs.prepend(QString::fromUtf8(envDir));
    }

    for (const QString &dirPath : fontDirs) {
        QDir dir(dirPath);
        if (dir.exists()) {
            QStringList filters;
            filters << QStringLiteral("*.ttf") << QStringLiteral("*.otf");
            const QStringList fontFiles = dir.entryList(filters, QDir::Files);
            for (const QString &f : fontFiles) {
                QFontDatabase::addApplicationFont(dir.filePath(f));
            }
            break;
        }
    }
}

void kynex_set_style_tokens(
    rust::Str surface,
    rust::Str surface_raised,
    rust::Str text,
    rust::Str text_muted,
    rust::Str accent,
    rust::Str danger,
    rust::Str warning,
    rust::Str success,
    rust::Str focus_ring,
    uint32_t spacing_unit,
    uint32_t spacing_small,
    uint32_t spacing_medium,
    uint32_t spacing_large,
    uint32_t spacing_panel,
    uint32_t radius_small,
    uint32_t radius_medium,
    uint32_t radius_large,
    uint32_t motion_fast_ms,
    uint32_t motion_normal_ms,
    uint32_t motion_slow_ms,
    rust::Str font_family,
    rust::Str font_fallback,
    uint32_t font_base_px,
    double opacity_disabled,
    double opacity_hover,
    double opacity_pressed,
    double opacity_focus
)
{
    Style *s = Style::instance();
    s->colors()->setColors(
        QString::fromUtf8(surface.data(), static_cast<qsizetype>(surface.size())),
        QString::fromUtf8(surface_raised.data(), static_cast<qsizetype>(surface_raised.size())),
        QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size())),
        QString::fromUtf8(text_muted.data(), static_cast<qsizetype>(text_muted.size())),
        QString::fromUtf8(accent.data(), static_cast<qsizetype>(accent.size())),
        QString::fromUtf8(danger.data(), static_cast<qsizetype>(danger.size())),
        QString::fromUtf8(warning.data(), static_cast<qsizetype>(warning.size())),
        QString::fromUtf8(success.data(), static_cast<qsizetype>(success.size())),
        QString::fromUtf8(focus_ring.data(), static_cast<qsizetype>(focus_ring.size()))
    );

    s->spacing()->setSpacing(
        static_cast<int>(spacing_unit),
        static_cast<int>(spacing_small),
        static_cast<int>(spacing_medium),
        static_cast<int>(spacing_large),
        static_cast<int>(spacing_panel)
    );
    s->radius()->setRadii(
        static_cast<int>(radius_small),
        static_cast<int>(radius_medium),
        static_cast<int>(radius_large)
    );
    s->motion()->setMotion(
        static_cast<int>(motion_fast_ms),
        static_cast<int>(motion_normal_ms),
        static_cast<int>(motion_slow_ms)
    );
    s->typography()->setTypography(
        QString::fromUtf8(font_family.data(), static_cast<qsizetype>(font_family.size())),
        QString::fromUtf8(font_fallback.data(), static_cast<qsizetype>(font_fallback.size())),
        static_cast<int>(font_base_px)
    );
    s->opacity()->setOpacity(
        opacity_disabled,
        opacity_hover,
        opacity_pressed,
        opacity_focus
    );
}

uint64_t kynex_steady_clock_ns()
{
    auto now = std::chrono::steady_clock::now().time_since_epoch();
    return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(now).count());
}
