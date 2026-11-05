#include "style.h"

StyleColors::StyleColors(QObject *parent)
    : QObject(parent)
    , m_surface("#14161c")
    , m_surfaceRaised("#1d2029")
    , m_text("#eceff4")
    , m_textMuted("#9aa3b2")
    , m_accent("#6aa6ff")
    , m_danger("#ff6b6b")
    , m_success("#4cd1a0")
{
}

void StyleColors::setColors(const QString &surface, const QString &surfaceRaised,
                           const QString &text, const QString &textMuted,
                           const QString &accent, const QString &danger,
                           const QString &success)
{
    m_surface = QColor(surface);
    m_surfaceRaised = QColor(surfaceRaised);
    m_text = QColor(text);
    m_textMuted = QColor(textMuted);
    m_accent = QColor(accent);
    m_danger = QColor(danger);
    m_success = QColor(success);
    Q_EMIT changed();
}

StyleSpacing::StyleSpacing(QObject *parent)
    : QObject(parent)
    , m_unit(4)
{
}

void StyleSpacing::setUnit(int unit)
{
    if (m_unit != unit) {
        m_unit = unit;
        Q_EMIT changed();
    }
}

StyleRadius::StyleRadius(QObject *parent)
    : QObject(parent)
    , m_small(6)
    , m_medium(12)
    , m_large(20)
{
}

void StyleRadius::setRadii(int small, int medium, int large)
{
    m_small = small;
    m_medium = medium;
    m_large = large;
    Q_EMIT changed();
}

StyleMotion::StyleMotion(QObject *parent)
    : QObject(parent)
    , m_fastMs(90)
    , m_normalMs(150)
    , m_slowMs(240)
{
}

void StyleMotion::setMotion(int fastMs, int normalMs, int slowMs)
{
    m_fastMs = fastMs;
    m_normalMs = normalMs;
    m_slowMs = slowMs;
    Q_EMIT changed();
}

StyleTypography::StyleTypography(QObject *parent)
    : QObject(parent)
    , m_family("Inter")
    , m_fallback("sans-serif")
    , m_basePx(14)
{
}

void StyleTypography::setTypography(const QString &family, const QString &fallback, int basePx)
{
    m_family = family;
    m_fallback = fallback;
    m_basePx = basePx;
    Q_EMIT changed();
}

Style::Style(QObject *parent)
    : QObject(parent)
    , m_colors(this)
    , m_spacing(this)
    , m_radius(this)
    , m_motion(this)
    , m_typography(this)
{
}

Style *Style::instance()
{
    static Style s_instance;
    return &s_instance;
}
