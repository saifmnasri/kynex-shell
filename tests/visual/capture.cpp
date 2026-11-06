#include <QGuiApplication>
#include <QQmlEngine>
#include <QQuickView>
#include <QQuickItem>
#include <QImage>
#include <QUrl>
#include <QString>
#include <QDir>
#include <iostream>
#include "../../crates/kynex-shell/cpp/style.h"

static void setDarkTokens() {
    Style::instance()->colors()->setColors(
        "#14161c", "#1d2029", "#eceff4", "#9aa3b2",
        "#6aa6ff", "#ff6b6b", "#e5a50a", "#4cd1a0", "#6aa6ff"
    );
    Style::instance()->spacing()->setSpacing(4, 8, 12, 16, 48);
    Style::instance()->radius()->setRadii(6, 12, 20);
    Style::instance()->motion()->setMotion(90, 150, 240);
    Style::instance()->typography()->setTypography("Inter", "sans-serif", 14);
    Style::instance()->opacity()->setOpacity(0.38, 0.08, 0.16, 1.0);
}

static void setLightTokens() {
    Style::instance()->colors()->setColors(
        "#f6f7fa", "#ffffff", "#14161c", "#5b6475",
        "#2f6fe0", "#d63c3c", "#c07d00", "#1f9d73", "#2f6fe0"
    );
    Style::instance()->spacing()->setSpacing(4, 8, 12, 16, 48);
    Style::instance()->radius()->setRadii(6, 12, 20);
    Style::instance()->motion()->setMotion(90, 150, 240);
    Style::instance()->typography()->setTypography("Inter", "sans-serif", 14);
    Style::instance()->opacity()->setOpacity(0.38, 0.08, 0.16, 1.0);
}

int main(int argc, char *argv[]) {
    if (argc < 3) {
        std::cerr << "Usage: " << argv[0] << " <variant> <output_png>" << std::endl;
        return 1;
    }

    std::string variant = argv[1];
    std::string outputPath = argv[2];

    QGuiApplication app(argc, argv);

    if (variant == "rtl-100") {
        app.setLayoutDirection(Qt::RightToLeft);
    } else {
        app.setLayoutDirection(Qt::LeftToRight);
    }

    if (variant.rfind("light", 0) == 0) {
        setLightTokens();
    } else {
        setDarkTokens();
    }

    qmlRegisterSingletonInstance("Kynex.Style", 1, 0, "Style", Style::instance());

    QQuickView view;
    view.engine()->addImportPath("crates/kynex-shell/qml");

    QString qmlPath = QDir::current().filePath("tests/visual/Gallery.qml");
    view.setSource(QUrl::fromLocalFile(qmlPath));

    view.setResizeMode(QQuickView::SizeViewToRootObject);
    view.show();

    // Process events to ensure layout and rendering pipeline finishes
    for (int i = 0; i < 10; ++i) {
        QCoreApplication::processEvents();
    }

    QImage img = view.grabWindow();
    if (img.isNull()) {
        std::cerr << "Failed to grab window image" << std::endl;
        return 1;
    }

    if (!img.save(QString::fromStdString(outputPath))) {
        std::cerr << "Failed to save image to " << outputPath << std::endl;
        return 1;
    }

    std::cout << "Successfully rendered " << variant << " to " << outputPath
              << " (" << img.width() << "x" << img.height() << ")" << std::endl;
    return 0;
}
