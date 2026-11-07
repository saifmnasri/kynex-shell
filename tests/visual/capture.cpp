#include <QGuiApplication>
#include <QFontDatabase>
#include <QQmlEngine>
#include <QQuickView>
#include <QQuickItem>
#include <QQuickImageProvider>
#include <QPixmap>
#include <QImage>
#include <QUrl>
#include <QString>
#include <QDir>
#include <cmath>
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

class DummyIconProvider : public QQuickImageProvider {
public:
    DummyIconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) {}
    QPixmap requestPixmap(const QString &, QSize *size, const QSize &requestedSize) override {
        int w = requestedSize.width() > 0 ? requestedSize.width() : 24;
        int h = requestedSize.height() > 0 ? requestedSize.height() : 24;
        if (size) *size = QSize(w, h);
        QPixmap pm(w, h);
        pm.fill(Qt::transparent);
        return pm;
    }
};

int main(int argc, char *argv[]) {
    if (argc < 3) {
        std::cerr << "Usage: " << argv[0] << " <variant> <output_png>" << std::endl;
        std::cerr << "   or: " << argv[0] << " compare <golden_png> <rendered_png> [tolerance]" << std::endl;
        return 1;
    }

    if (std::string(argv[1]) == "compare") {
        if (argc < 4) {
            std::cerr << "Usage: " << argv[0] << " compare <golden_png> <rendered_png> [tolerance]" << std::endl;
            return 1;
        }
        std::string goldenPath = argv[2];
        std::string renderedPath = argv[3];
        double tolerance = argc > 4 ? std::stod(argv[4]) : 1.0;

        QImage golden(QString::fromStdString(goldenPath));
        QImage rendered(QString::fromStdString(renderedPath));
        if (golden.isNull() || rendered.isNull()) {
            std::cerr << "ERROR: Failed to load comparison images." << std::endl;
            return 1;
        }
        if (golden.size() != rendered.size()) {
            std::cerr << "ERROR: Size mismatch: " << golden.width() << "x" << golden.height()
                      << " vs " << rendered.width() << "x" << rendered.height() << std::endl;
            return 1;
        }

        QImage gRgba = golden.convertToFormat(QImage::Format_RGBA8888);
        QImage rRgba = rendered.convertToFormat(QImage::Format_RGBA8888);

        quint64 sumSq = 0;
        const uchar *p1 = gRgba.constBits();
        const uchar *p2 = rRgba.constBits();
        qsizetype totalBytes = gRgba.sizeInBytes();

        for (qsizetype i = 0; i < totalBytes; ++i) {
            int diff = std::abs((int)p1[i] - (int)p2[i]);
            sumSq += diff * diff;
        }

        double rms = std::sqrt((double)sumSq / (double)totalBytes);
        std::cout << "RMS delta: " << rms << " (tolerance: " << tolerance << ")" << std::endl;

        if (rms > tolerance) {
            std::string diffPath = argc > 5 ? argv[5] : "";
            if (!diffPath.empty()) {
                QImage diffImg(golden.size(), QImage::Format_RGBA8888);
                for (int y = 0; y < golden.height(); ++y) {
                    for (int x = 0; x < golden.width(); ++x) {
                        QRgb pg = gRgba.pixel(x, y);
                        QRgb pr = rRgba.pixel(x, y);
                        int dr = std::abs(qRed(pg) - qRed(pr));
                        int dg = std::abs(qGreen(pg) - qGreen(pr));
                        int db = std::abs(qBlue(pg) - qBlue(pr));
                        int da = std::abs(qAlpha(pg) - qAlpha(pr));
                        if (dr == 0 && dg == 0 && db == 0 && da == 0) {
                            diffImg.setPixel(x, y, qRgba(0, 0, 0, 255));
                        } else {
                            diffImg.setPixel(x, y, qRgba(
                                std::min(255, dr * 10 + 64),
                                std::min(255, dg * 10),
                                std::min(255, db * 10 + 64),
                                255
                            ));
                        }
                    }
                }
                if (!diffImg.save(QString::fromStdString(diffPath))) {
                    std::cerr << "WARNING: Failed to save diff image to " << diffPath << std::endl;
                }
            }
            std::cerr << "FAIL: RMS delta " << rms << " exceeds tolerance " << tolerance << std::endl;
            return 1;
        }
        return 0;
    }

    std::string variant = argv[1];
    std::string outputPath = argv[2];

    QGuiApplication app(argc, argv);

    // Load bundled application fonts
    QDir fontsDir("assets/fonts");
    if (!fontsDir.exists()) {
        fontsDir = QDir("../assets/fonts");
    }
    if (fontsDir.exists()) {
        const QStringList fontFiles = fontsDir.entryList(QStringList() << "*.ttf" << "*.otf", QDir::Files);
        for (const QString &f : fontFiles) {
            QFontDatabase::addApplicationFont(fontsDir.filePath(f));
        }
    }

    if (variant == "rtl-100" || variant == "launcher-rtl") {
        app.setLayoutDirection(Qt::RightToLeft);
        QLocale::setDefault(QLocale(QLocale::Arabic));
    } else {
        app.setLayoutDirection(Qt::LeftToRight);
        QLocale::setDefault(QLocale(QLocale::English));
    }

    if (variant.rfind("light", 0) == 0) {
        setLightTokens();
    } else {
        setDarkTokens();
    }

    qmlRegisterSingletonInstance("Kynex.Style", 1, 0, "Style", Style::instance());

    QQuickView view;
    view.engine()->addImageProvider(QStringLiteral("kynex-icon"), new DummyIconProvider());
    view.engine()->addImportPath("crates/kynex-shell/qml");

    QString qmlFile = "tests/visual/Gallery.qml";
    QString scenario = "results";
    if (variant == "launcher-results") {
        qmlFile = "tests/visual/LauncherHarness.qml";
        scenario = "results";
    } else if (variant == "launcher-no-results") {
        qmlFile = "tests/visual/LauncherHarness.qml";
        scenario = "no-results";
    } else if (variant == "launcher-empty") {
        qmlFile = "tests/visual/LauncherHarness.qml";
        scenario = "empty";
    } else if (variant == "launcher-rtl") {
        qmlFile = "tests/visual/LauncherHarness.qml";
        scenario = "rtl";
    }

    QString qmlPath = QDir::current().filePath(qmlFile);
    view.setSource(QUrl::fromLocalFile(qmlPath));

    if (auto *rootObj = view.rootObject()) {
        rootObj->setProperty("scenario", scenario);
    }

    view.setResizeMode(QQuickView::SizeViewToRootObject);
    view.show();
    view.requestActivate();

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
