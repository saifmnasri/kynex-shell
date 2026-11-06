#pragma once

#include <QColor>
#include <QObject>
#include <QString>

class StyleColors : public QObject {
    Q_OBJECT
    Q_PROPERTY(QColor surface READ surface NOTIFY changed FINAL)
    Q_PROPERTY(QColor surfaceRaised READ surfaceRaised NOTIFY changed FINAL)
    Q_PROPERTY(QColor text READ text NOTIFY changed FINAL)
    Q_PROPERTY(QColor textMuted READ textMuted NOTIFY changed FINAL)
    Q_PROPERTY(QColor accent READ accent NOTIFY changed FINAL)
    Q_PROPERTY(QColor danger READ danger NOTIFY changed FINAL)
    Q_PROPERTY(QColor warning READ warning NOTIFY changed FINAL)
    Q_PROPERTY(QColor success READ success NOTIFY changed FINAL)

public:
    explicit StyleColors(QObject *parent = nullptr);

    QColor surface() const { return m_surface; }
    QColor surfaceRaised() const { return m_surfaceRaised; }
    QColor text() const { return m_text; }
    QColor textMuted() const { return m_textMuted; }
    QColor accent() const { return m_accent; }
    QColor danger() const { return m_danger; }
    QColor warning() const { return m_warning; }
    QColor success() const { return m_success; }

    void setColors(const QString &surface, const QString &surfaceRaised,
                   const QString &text, const QString &textMuted,
                   const QString &accent, const QString &danger,
                   const QString &warning, const QString &success);

Q_SIGNALS:
    void changed();

private:
    QColor m_surface;
    QColor m_surfaceRaised;
    QColor m_text;
    QColor m_textMuted;
    QColor m_accent;
    QColor m_danger;
    QColor m_warning;
    QColor m_success;
};

class StyleSpacing : public QObject {
    Q_OBJECT
    Q_PROPERTY(int unit READ unit NOTIFY changed FINAL)
    Q_PROPERTY(int small READ small NOTIFY changed FINAL)
    Q_PROPERTY(int medium READ medium NOTIFY changed FINAL)
    Q_PROPERTY(int large READ large NOTIFY changed FINAL)
    Q_PROPERTY(int panel READ panel NOTIFY changed FINAL)

public:
    explicit StyleSpacing(QObject *parent = nullptr);

    int unit() const { return m_unit; }
    int small() const { return m_small; }
    int medium() const { return m_medium; }
    int large() const { return m_large; }
    int panel() const { return m_panel; }

    void setSpacing(int unit, int small, int medium, int large, int panel);

Q_SIGNALS:
    void changed();

private:
    int m_unit = 4;
    int m_small = 8;
    int m_medium = 12;
    int m_large = 16;
    int m_panel = 48;
};

class StyleRadius : public QObject {
    Q_OBJECT
    Q_PROPERTY(int small READ small NOTIFY changed FINAL)
    Q_PROPERTY(int medium READ medium NOTIFY changed FINAL)
    Q_PROPERTY(int large READ large NOTIFY changed FINAL)

public:
    explicit StyleRadius(QObject *parent = nullptr);

    int small() const { return m_small; }
    int medium() const { return m_medium; }
    int large() const { return m_large; }

    void setRadii(int small, int medium, int large);

Q_SIGNALS:
    void changed();

private:
    int m_small = 6;
    int m_medium = 12;
    int m_large = 20;
};

class StyleMotion : public QObject {
    Q_OBJECT
    Q_PROPERTY(int fastMs READ fastMs NOTIFY changed FINAL)
    Q_PROPERTY(int normalMs READ normalMs NOTIFY changed FINAL)
    Q_PROPERTY(int slowMs READ slowMs NOTIFY changed FINAL)

public:
    explicit StyleMotion(QObject *parent = nullptr);

    int fastMs() const { return m_fastMs; }
    int normalMs() const { return m_normalMs; }
    int slowMs() const { return m_slowMs; }

    void setMotion(int fastMs, int normalMs, int slowMs);

Q_SIGNALS:
    void changed();

private:
    int m_fastMs = 90;
    int m_normalMs = 150;
    int m_slowMs = 240;
};

class StyleTypography : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString family READ family NOTIFY changed FINAL)
    Q_PROPERTY(QString fallback READ fallback NOTIFY changed FINAL)
    Q_PROPERTY(int basePx READ basePx NOTIFY changed FINAL)

public:
    explicit StyleTypography(QObject *parent = nullptr);

    QString family() const { return m_family; }
    QString fallback() const { return m_fallback; }
    int basePx() const { return m_basePx; }

    void setTypography(const QString &family, const QString &fallback, int basePx);

Q_SIGNALS:
    void changed();

private:
    QString m_family = QStringLiteral("Inter");
    QString m_fallback = QStringLiteral("sans-serif");
    int m_basePx = 14;
};

class Style : public QObject {
    Q_OBJECT
    Q_PROPERTY(StyleColors* colors READ colors CONSTANT FINAL)
    Q_PROPERTY(StyleSpacing* spacing READ spacing CONSTANT FINAL)
    Q_PROPERTY(StyleRadius* radius READ radius CONSTANT FINAL)
    Q_PROPERTY(StyleMotion* motion READ motion CONSTANT FINAL)
    Q_PROPERTY(StyleTypography* typography READ typography CONSTANT FINAL)

public:
    static Style *instance();

    StyleColors *colors() { return &m_colors; }
    StyleSpacing *spacing() { return &m_spacing; }
    StyleRadius *radius() { return &m_radius; }
    StyleMotion *motion() { return &m_motion; }
    StyleTypography *typography() { return &m_typography; }

private:
    explicit Style(QObject *parent = nullptr);

    StyleColors m_colors;
    StyleSpacing m_spacing;
    StyleRadius m_radius;
    StyleMotion m_motion;
    StyleTypography m_typography;
};
