#include "shim.h"
#include "layersurface.h"
#include "style.h"
#include <QtQml>

void kynex_register_core_types()
{
    qmlRegisterType<LayerSurface>("Kynex.Shell", 1, 0, "LayerSurface");
    qmlRegisterSingletonInstance<Style>("Kynex.Style", 1, 0, "Style", Style::instance());
}

void kynex_set_style_tokens(
    rust::Str surface,
    rust::Str surface_raised,
    rust::Str text,
    rust::Str text_muted,
    rust::Str accent,
    rust::Str danger,
    rust::Str success,
    uint32_t spacing_unit,
    uint32_t radius_small,
    uint32_t radius_medium,
    uint32_t radius_large,
    uint32_t motion_fast_ms,
    uint32_t motion_normal_ms,
    uint32_t motion_slow_ms,
    rust::Str font_family,
    rust::Str font_fallback,
    uint32_t font_base_px
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
        QString::fromUtf8(success.data(), static_cast<qsizetype>(success.size()))
    );

    s->spacing()->setUnit(static_cast<int>(spacing_unit));
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
}
