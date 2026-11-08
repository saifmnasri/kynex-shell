#include <QtQuickTest/quicktest.h>
#include <QQmlEngine>
#include <QQmlContext>
#include <QGuiApplication>
#include <QLocale>
#include "../../crates/kynex-shell/cpp/style.h"

class LayoutHelper : public QObject {
    Q_OBJECT
public:
    explicit LayoutHelper(QObject *parent = nullptr) : QObject(parent) {}
    Q_INVOKABLE void setRtl(bool rtl) {
        if (rtl) {
            QLocale::setDefault(QLocale(QLocale::Arabic, QLocale::SaudiArabia));
            QGuiApplication::setLayoutDirection(Qt::RightToLeft);
        } else {
            QLocale::setDefault(QLocale::c());
            QGuiApplication::setLayoutDirection(Qt::LeftToRight);
        }
    }
};

class Setup : public QObject {
    Q_OBJECT
public:
    Setup() {}
public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        Style::instance()->colors()->setColors(
            "#14161c", "#1d2029", "#eceff4", "#9aa3b2",
            "#6aa6ff", "#ff6b6b", "#e5a50a", "#4cd1a0", "#6aa6ff"
        );
        Style::instance()->spacing()->setSpacing(4, 8, 12, 16, 48);
        Style::instance()->radius()->setRadii(6, 12, 20);
        Style::instance()->motion()->setMotion(90, 150, 240);
        Style::instance()->typography()->setTypography("Inter", "sans-serif", 14);
        Style::instance()->opacity()->setOpacity(0.38, 0.08, 0.16, 1.0);
        qmlRegisterSingletonInstance("Kynex.Style", 1, 0, "Style", Style::instance());
        engine->addImportPath("crates/kynex-shell/qml");
        engine->rootContext()->setContextProperty("LayoutHelper", new LayoutHelper(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(controls, Setup)
#include "main.moc"
