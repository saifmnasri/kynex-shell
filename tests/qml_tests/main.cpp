#include <QtQuickTest/quicktest.h>
#include <QQmlEngine>
#include <QQmlContext>
#include <QGuiApplication>
#include <QFontDatabase>
#include <QDir>
#include <QLocale>
#include <QAbstractListModel>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QQuickItem>
#include <QQuickImageProvider>
#include <QPixmap>
#include <iostream>
#include <vector>
#include <atomic>
#include <mutex>
#include "../../crates/kynex-shell/cpp/style.h"
#include "../../crates/kynex-shell/cpp/layersurface.h"
#include "mocks.h"

static std::atomic<int> s_qmlWarningGateViolations{0};
static std::vector<std::string> s_qmlWarningGateViolationMessages;
static std::mutex s_qmlWarningGateMutex;

void qmlWarningGateMessageHandler(QtMsgType type, const QMessageLogContext &context, const QString &msg)
{
    // Allowlist: at most 3 justified entries
    // 1. LayerShellQt offscreen notice when Wayland compositor is absent
    if (msg.contains(QStringLiteral("is not a wayland window. Not creating zwlr_layer_surface"))) {
        return;
    }
    // 2. Fontconfig warning in offscreen / container environments
    if (msg.contains(QStringLiteral("Fontconfig error")) || msg.contains(QStringLiteral("Fontconfig warning"))) {
        return;
    }
    // 3. QThreadStorage cleanup notice during thread teardown
    if (msg.contains(QStringLiteral("QThreadStorage: entry"))) {
        return;
    }

    bool violation = false;
    static const QStringList bannedPatterns = {
        QStringLiteral("Detected function"),
        QStringLiteral("no signal"),
        QStringLiteral("ReferenceError"),
        QStringLiteral("TypeError"),
        QStringLiteral("Unable to assign"),
        QStringLiteral("Binding loop"),
        QStringLiteral("is not a type"),
        QStringLiteral("Cannot read property"),
        QStringLiteral("is not installed")
    };

    for (const QString &banned : bannedPatterns) {
        if (msg.contains(banned, Qt::CaseInsensitive)) {
            violation = true;
            break;
        }
    }

    // Flag any QtWarningMsg or QtCriticalMsg coming from our QML files
    if (type == QtWarningMsg || type == QtCriticalMsg) {
        if (msg.contains(QStringLiteral(".qml"), Qt::CaseInsensitive) ||
            (context.file && strstr(context.file, ".qml") != nullptr)) {
            violation = true;
        }
    }

    if (violation) {
        s_qmlWarningGateViolations++;
        std::lock_guard<std::mutex> lock(s_qmlWarningGateMutex);
        s_qmlWarningGateViolationMessages.push_back(msg.toStdString());
        std::cerr << "[QML WARNING GATE VIOLATION] " << msg.toStdString() << "\n";
    }

    QByteArray localMsg = msg.toLocal8Bit();
    fprintf(stderr, "%s\n", localMsg.constData());
}

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
    Q_INVOKABLE void sendKey(QQuickItem *item, int key, int modifiers = 0) {
        if (!item) return;
        QKeyEvent press(QEvent::KeyPress, key, static_cast<Qt::KeyboardModifiers>(modifiers));
        QCoreApplication::sendEvent(item, &press);
        QKeyEvent release(QEvent::KeyRelease, key, static_cast<Qt::KeyboardModifiers>(modifiers));
        QCoreApplication::sendEvent(item, &release);
    }
    Q_INVOKABLE void sendMouseClick(QQuickItem *item, qreal x, qreal y, int button = Qt::LeftButton) {
        if (!item) return;
        QPointF pos(x, y);
        auto btn = static_cast<Qt::MouseButton>(button);
        QMouseEvent press(QEvent::MouseButtonPress, pos, pos, btn, btn, Qt::NoModifier);
        QCoreApplication::sendEvent(item, &press);
        QMouseEvent release(QEvent::MouseButtonRelease, pos, pos, btn, Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(item, &release);
    }
    Q_INVOKABLE void sendMouseMove(QQuickItem *item, qreal x, qreal y) {
        if (!item) return;
        QPointF pos(x, y);
        QHoverEvent hoverEnter(QEvent::HoverEnter, pos, pos, QPointF(-1, -1));
        QCoreApplication::sendEvent(item, &hoverEnter);
        QHoverEvent hoverMove(QEvent::HoverMove, pos, pos, pos);
        QCoreApplication::sendEvent(item, &hoverMove);
        QEnterEvent enter(pos, pos, pos);
        QCoreApplication::sendEvent(item, &enter);
        QMouseEvent move(QEvent::MouseMove, pos, pos, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(item, &move);
    }
    Q_INVOKABLE void sendMouseLeave(QQuickItem *item) {
        if (!item) return;
        QPointF outPos(-100, -100);
        QPointF oldPos(0, 0);
        QHoverEvent hoverLeave(QEvent::HoverLeave, outPos, outPos, oldPos);
        QCoreApplication::sendEvent(item, &hoverLeave);
        QEvent leave(QEvent::Leave);
        QCoreApplication::sendEvent(item, &leave);
    }
    Q_INVOKABLE void sendWheel(QQuickItem *item, qreal x, qreal y, int deltaY, int deltaX = 0) {
        if (!item) return;
        QPointF pos(x, y);
        QPoint pixelDelta;
        QPoint angleDelta(deltaX, deltaY);
        QWheelEvent wheel(pos, pos, pixelDelta, angleDelta, Qt::NoButton, Qt::NoModifier, Qt::NoScrollPhase, false);
        QCoreApplication::sendEvent(item, &wheel);
    }
};

class DummyIconProvider : public QQuickImageProvider {
public:
    DummyIconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) {}
    QPixmap requestPixmap(const QString &, QSize *size, const QSize &requestedSize) override {
        int w = requestedSize.width() > 0 ? requestedSize.width() : 24;
        int h = requestedSize.height() > 0 ? requestedSize.height() : 24;
        if (size) *size = QSize(w, h);
        QPixmap pix(w, h);
        pix.fill(Qt::transparent);
        return pix;
    }
};

class Setup : public QObject {
    Q_OBJECT
public:
    Setup() {
        qInstallMessageHandler(qmlWarningGateMessageHandler);
    }
    ~Setup() override {
        int violations = s_qmlWarningGateViolations.load();
        if (violations > 0) {
            std::cerr << "\n=======================================================\n"
                      << "FATAL: QML WARNING GATE FAILED WITH " << violations << " VIOLATION(S):\n";
            for (const auto &m : s_qmlWarningGateViolationMessages) {
                std::cerr << "  - " << m << "\n";
            }
            std::cerr << "=======================================================\n";
            std::exit(1);
        }
    }
public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        engine->addImageProvider(QStringLiteral("kynex-icon"), new DummyIconProvider());
        engine->addImageProvider(QStringLiteral("kynex-tray"), new DummyIconProvider());
        QDir fontsDir("assets/fonts");
        if (fontsDir.exists()) {
            const QStringList fontFiles = fontsDir.entryList(QStringList() << "*.ttf" << "*.otf", QDir::Files);
            for (const QString &f : fontFiles) {
                QFontDatabase::addApplicationFont(fontsDir.filePath(f));
            }
        }
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
        qmlRegisterType<LayerSurface>("Kynex.Shell", 1, 0, "LayerSurface");
        qmlRegisterType<MockLauncherModel>("Kynex.Shell", 1, 0, "LauncherModel");
        qmlRegisterType<MockClock>("Kynex.Shell", 1, 0, "Clock");
        qmlRegisterType<MockPowerModel>("Kynex.Shell", 1, 0, "PowerModel");
        qmlRegisterType<MockNetworkModel>("Kynex.Shell", 1, 0, "NetworkModel");
        qmlRegisterType<MockAudioModel>("Kynex.Shell", 1, 0, "AudioModel");
        qmlRegisterType<MockSessionModel>("Kynex.Shell", 1, 0, "SessionModel");
        qmlRegisterType<MockWindowListModel>("Kynex.Shell", 1, 0, "WindowListModel");
        qmlRegisterType<MockNotificationModel>("Kynex.Shell", 1, 0, "NotificationModel");
        qmlRegisterType<MockTrayModel>("Kynex.Shell", 1, 0, "TrayModel");
        qmlRegisterType<MockTrayMenuModel>("Kynex.Shell", 1, 0, "TrayMenuModel");
        engine->addImportPath("crates/kynex-shell/qml");
        engine->rootContext()->setContextProperty("LayoutHelper", new LayoutHelper(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(controls, Setup)
#include "main.moc"
