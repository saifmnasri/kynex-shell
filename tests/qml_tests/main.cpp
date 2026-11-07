#include <QtQuickTest/quicktest.h>
#include <QQmlEngine>
#include <QGuiApplication>
#include "../../crates/kynex-shell/cpp/style.h"

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
    }
};

QUICK_TEST_MAIN_WITH_SETUP(controls, Setup)
#include "main.moc"
