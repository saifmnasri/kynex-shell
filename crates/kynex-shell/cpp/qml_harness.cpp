#include "qml_harness.h"
#include "shim.h"
#include "style.h"
#include "layersurface.h"

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickItem>
#include <QSignalSpy>
#include <QAbstractItemModel>
#include <QDBusInterface>
#include <QDBusReply>
#include <QDBusConnection>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDir>
#include <QElapsedTimer>
#include <QThread>
#include <QScreen>
#include <QDateTime>
#include <iostream>
#include <vector>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>

static QQuickWindow *s_globalPanelWindow = nullptr;

void kynex_set_panel_window(QQuickWindow *window)
{
    s_globalPanelWindow = window;
}

void kynex_register_engine_panel(QQmlApplicationEngine &engine)
{
    if (!engine.rootObjects().isEmpty()) {
        auto *window = qobject_cast<QQuickWindow*>(engine.rootObjects().first());
        if (window) {
            kynex_set_panel_window(window);
        }
    }
}

QQuickWindow *kynex_get_panel_window()
{
    return s_globalPanelWindow;
}

static void waitEvents(int ms)
{
    QElapsedTimer timer;
    timer.start();
    while (timer.elapsed() < ms) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 50);
        QThread::msleep(10);
    }
}

static QJsonObject dumpItemRecursive(QQuickItem *item, int depth)
{
    QJsonObject obj;
    if (!item || depth > 20) {
        return obj;
    }

    obj[QStringLiteral("objectName")] = item->objectName();
    obj[QStringLiteral("visible")] = item->isVisible();
    obj[QStringLiteral("x")] = item->x();
    obj[QStringLiteral("y")] = item->y();
    obj[QStringLiteral("width")] = item->width();
    obj[QStringLiteral("height")] = item->height();
    obj[QStringLiteral("opacity")] = item->opacity();

    QQuickItem *parent = item->parentItem();
    QJsonArray parentSize;
    parentSize.append(parent ? parent->width() : 0.0);
    parentSize.append(parent ? parent->height() : 0.0);
    obj[QStringLiteral("parentSize")] = parentSize;

    QJsonArray children;
    for (QQuickItem *child : item->childItems()) {
        children.append(dumpItemRecursive(child, depth + 1));
    }
    obj[QStringLiteral("children")] = children;

    return obj;
}

rust::String kynex_dump_scene_json(const QQuickWindow *window)
{
    if (!window) {
        return rust::String("{\"error\":\"no window\"}");
    }

    QJsonObject root;
    QJsonObject panelGeom;
    panelGeom[QStringLiteral("x")] = window->x();
    panelGeom[QStringLiteral("y")] = window->y();
    panelGeom[QStringLiteral("width")] = window->width();
    panelGeom[QStringLiteral("height")] = window->height();
    root[QStringLiteral("panelGeometry")] = panelGeom;

    QJsonObject winSize;
    winSize[QStringLiteral("width")] = window->width();
    winSize[QStringLiteral("height")] = window->height();
    root[QStringLiteral("windowSize")] = winSize;

    root[QStringLiteral("devicePixelRatio")] = window->devicePixelRatio();

    QJsonObject screenGeom;
    if (window->screen()) {
        QRect sg = window->screen()->geometry();
        screenGeom[QStringLiteral("x")] = sg.x();
        screenGeom[QStringLiteral("y")] = sg.y();
        screenGeom[QStringLiteral("width")] = sg.width();
        screenGeom[QStringLiteral("height")] = sg.height();
    } else {
        screenGeom[QStringLiteral("width")] = window->width();
        screenGeom[QStringLiteral("height")] = window->height();
    }
    root[QStringLiteral("screenGeometry")] = screenGeom;

    // Delegate and model counts
    QObject *model = window->property("windowListModel").value<QObject*>();
    auto *itemModel = qobject_cast<QAbstractItemModel*>(model);
    root[QStringLiteral("modelRowCount")] = itemModel ? itemModel->rowCount() : -1;

    auto *taskbarList = window->property("taskbarList").value<QQuickItem*>();
    int delegateCount = 0;
    if (taskbarList) {
        root[QStringLiteral("taskbarListVisible")] = taskbarList->property("visible").toBool();
        root[QStringLiteral("taskbarListCount")] = taskbarList->property("count").toInt();
        auto *contentItem = taskbarList->property("contentItem").value<QQuickItem*>();
        if (contentItem) {
            delegateCount = contentItem->childItems().size();
        }
    }
    root[QStringLiteral("delegateCount")] = delegateCount;

    QQuickItem *rootItem = window->contentItem();
    if (rootItem) {
        root[QStringLiteral("items")] = dumpItemRecursive(rootItem, 0);
    }

    QJsonDocument doc(root);
    QByteArray bytes = doc.toJson(QJsonDocument::Compact);
    return rust::String(bytes.constData(), bytes.size());
}

rust::String kynex_grab_panel_image(QQuickWindow *window)
{
    if (!window) {
        return rust::String("");
    }

    QImage img = window->grabWindow();
    if (img.isNull()) {
        std::cerr << "kynex_grab_panel_image: grabWindow returned null image\n";
        return rust::String("");
    }

    const char *xdgRuntime = getenv("XDG_RUNTIME_DIR");
    QString baseDir = xdgRuntime ? QString::fromUtf8(xdgRuntime) : QStringLiteral("/tmp");
    QString kynexDir = baseDir + QStringLiteral("/kynex");
    QDir().mkpath(kynexDir);

    qint64 now = QDateTime::currentMSecsSinceEpoch();
    QString filePath = QStringLiteral("%1/panel-%2.png").arg(kynexDir).arg(now);

    QByteArray pathBytes = filePath.toUtf8();
    int fd = open(pathBytes.constData(), O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) {
        std::cerr << "kynex_grab_panel_image: open failed for " << pathBytes.constData() << "\n";
        return rust::String("");
    }
    close(fd);

    if (!img.save(filePath, "PNG")) {
        std::cerr << "kynex_grab_panel_image: save failed for " << pathBytes.constData() << "\n";
        return rust::String("");
    }
    chmod(pathBytes.constData(), 0600);

    return rust::String(pathBytes.constData(), pathBytes.size());
}

rust::String kynex_dump_active_scene()
{
    return kynex_dump_scene_json(s_globalPanelWindow);
}

rust::String kynex_grab_active_panel()
{
    return kynex_grab_panel_image(s_globalPanelWindow);
}

int kynex_run_taskbar_qml_harness(rust::Str destdir, int32_t width, int32_t height, double dpr)
{
    QString destdirStr = QString::fromUtf8(destdir.data(), static_cast<qsizetype>(destdir.size()));
    std::cout << "[QML HARNESS] Starting harness with DESTDIR=" << destdirStr.toStdString()
              << " geom=" << width << "x" << height << "@" << dpr << std::endl;

    static int argc = 1;
    static char arg0[] = "qml_taskbar_harness";
    static char *argv[] = { arg0, nullptr };

    qputenv("QT_QPA_PLATFORM", QByteArray("offscreen"));
    qputenv("QT_QUICK_BACKEND", QByteArray("software"));
    if (dpr > 1.0) {
        qputenv("QT_SCALE_FACTOR", QByteArray::number(dpr));
    }

    if (!QCoreApplication::instance()) {
        new QGuiApplication(argc, argv);
    }
    QGuiApplication::setDesktopFileName(QStringLiteral("org.kynexos.Shell"));

    kynex_init_security_limits();
    kynex_load_bundled_fonts();
    kynex_register_core_types();

    // Load design tokens
    QString styleJsonPath = destdirStr + QStringLiteral("/usr/share/kynex/styles/default/style.json");
    if (QFile::exists(styleJsonPath)) {
        QFile f(styleJsonPath);
        if (f.open(QIODevice::ReadOnly)) {
            QJsonDocument doc = QJsonDocument::fromJson(f.readAll());
            QJsonObject root = doc.object();
            QJsonObject colors = root[QStringLiteral("colors")].toObject();
            QJsonObject spacing = root[QStringLiteral("spacing")].toObject();
            QJsonObject radius = root[QStringLiteral("radius")].toObject();
            QJsonObject motion = root[QStringLiteral("motion")].toObject();
            QJsonObject typography = root[QStringLiteral("typography")].toObject();
            QJsonObject opacity = root[QStringLiteral("opacity")].toObject();

            Style::instance()->colors()->setColors(
                colors[QStringLiteral("surface")].toString(QStringLiteral("#14161c")),
                colors[QStringLiteral("surface_raised")].toString(QStringLiteral("#1d2029")),
                colors[QStringLiteral("text")].toString(QStringLiteral("#eceff4")),
                colors[QStringLiteral("text_muted")].toString(QStringLiteral("#9aa3b2")),
                colors[QStringLiteral("accent")].toString(QStringLiteral("#6aa6ff")),
                colors[QStringLiteral("danger")].toString(QStringLiteral("#ff6b6b")),
                colors[QStringLiteral("warning")].toString(QStringLiteral("#e5a50a")),
                colors[QStringLiteral("success")].toString(QStringLiteral("#4cd1a0")),
                colors[QStringLiteral("focus_ring")].toString(QStringLiteral("#6aa6ff"))
            );
            Style::instance()->spacing()->setSpacing(
                spacing[QStringLiteral("unit")].toInt(4),
                spacing[QStringLiteral("small")].toInt(8),
                spacing[QStringLiteral("medium")].toInt(12),
                spacing[QStringLiteral("large")].toInt(16),
                spacing[QStringLiteral("panel")].toInt(48)
            );
            Style::instance()->radius()->setRadii(
                radius[QStringLiteral("small")].toInt(6),
                radius[QStringLiteral("medium")].toInt(12),
                radius[QStringLiteral("large")].toInt(20)
            );
            Style::instance()->motion()->setMotion(
                motion[QStringLiteral("fast_ms")].toInt(90),
                motion[QStringLiteral("normal_ms")].toInt(150),
                motion[QStringLiteral("slow_ms")].toInt(240)
            );
            Style::instance()->typography()->setTypography(
                typography[QStringLiteral("family")].toString(QStringLiteral("Inter")),
                typography[QStringLiteral("fallback")].toString(QStringLiteral("sans-serif")),
                typography[QStringLiteral("base_px")].toInt(14)
            );
            Style::instance()->opacity()->setOpacity(
                opacity[QStringLiteral("disabled")].toDouble(0.38),
                opacity[QStringLiteral("hover")].toDouble(0.08),
                opacity[QStringLiteral("pressed")].toDouble(0.16),
                opacity[QStringLiteral("focus")].toDouble(1.0)
            );
        }
    }

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(
        QStringLiteral("kynexMutantZeroWidth"),
        qEnvironmentVariableIsSet("KYNEX_TEST_MUTANT_ZERO_WIDTH")
    );
    engine.rootContext()->setContextProperty(
        QStringLiteral("kynexMutantHiddenDelegate"),
        qEnvironmentVariableIsSet("KYNEX_TEST_MUTANT_HIDDEN_DELEGATE")
    );
    kynex_register_image_provider(engine);

    QString componentsDir = destdirStr + QStringLiteral("/usr/share/kynex/styles/default/components");
    if (QDir(componentsDir).exists()) {
        engine.addImportPath(componentsDir);
    }
    QString qmlDir = destdirStr + QStringLiteral("/usr/share/kynex/qml");
    if (QDir(qmlDir).exists()) {
        engine.addImportPath(qmlDir);
    }

    QString shellQml = destdirStr + QStringLiteral("/usr/share/kynex/styles/default/Shell.qml");
    if (!QFile::exists(shellQml)) {
        std::cerr << "[QML HARNESS ERROR] Shell.qml not found at: " << shellQml.toStdString() << std::endl;
        return 1;
    }

    engine.load(QUrl::fromLocalFile(shellQml));
    waitEvents(200);

    if (engine.rootObjects().isEmpty()) {
        std::cerr << "[QML HARNESS ERROR] Failed to load Shell.qml - rootObjects is empty" << std::endl;
        return 1;
    }

    auto *panelWindow = qobject_cast<QQuickWindow*>(engine.rootObjects().first());
    if (!panelWindow) {
        std::cerr << "[QML HARNESS ERROR] Root object is not a QQuickWindow" << std::endl;
        return 1;
    }

    kynex_set_panel_window(panelWindow);

    panelWindow->setWidth(width);
    panelWindow->setProperty("screenWidth", width);
    panelWindow->setProperty("screenHeight", height);
    panelWindow->setProperty("screenDevicePixelRatio", dpr);
    panelWindow->show();
    waitEvents(100);

    QObject *modelObj = panelWindow->property("windowListModel").value<QObject*>();
    auto *model = qobject_cast<QAbstractItemModel*>(modelObj);
    if (!model) {
        std::cerr << "[QML HARNESS ERROR] windowListModel not found or not QAbstractItemModel" << std::endl;
        return 1;
    }

    auto *taskbarList = panelWindow->property("taskbarList").value<QQuickItem*>();
    if (!taskbarList) {
        std::cerr << "[QML HARNESS ERROR] taskbarList item not found on panel" << std::endl;
        return 1;
    }

    auto *statusArea = panelWindow->property("statusArea").value<QQuickItem*>();
    if (!statusArea) {
        std::cerr << "[QML HARNESS ERROR] statusArea item not found on panel" << std::endl;
        return 1;
    }

    QQuickItem *clockRow = panelWindow->findChild<QQuickItem*>(QStringLiteral("clockRow"));
    if (!clockRow) {
        // Fallback search in panel children for Row containing clock texts
        for (QQuickItem *item : panelWindow->contentItem()->childItems()) {
            if (item && item->inherits("QQuickRow")) {
                clockRow = item;
                break;
            }
        }
    }

    // Attach QSignalSpy
    QSignalSpy spyReset(model, &QAbstractItemModel::modelReset);
    QSignalSpy spyRows(model, &QAbstractItemModel::rowsInserted);
    QSignalSpy spyData(model, &QAbstractItemModel::dataChanged);
    QSignalSpy spyCount(model, SIGNAL(countChanged()));

    // Setup D-Bus feeder proxy
    QDBusInterface feeder(
        QStringLiteral("org.kynexos.Daemon"),
        QStringLiteral("/org/kynexos/Daemon/Windows"),
        QStringLiteral("org.kynexos.Daemon.WindowsFeed1"),
        QDBusConnection::sessionBus()
    );

    if (!feeder.isValid()) {
        std::cerr << "[QML HARNESS ERROR] WindowsFeed1 interface not available on session bus: "
                  << feeder.lastError().message().toStdString() << std::endl;
        return 1;
    }

    // Verification Lambda
    auto verifyState = [&](int expectedCount, const QString &stepName, int prevReset, int prevRows, int prevData) -> bool {
        // Wait up to 2 seconds for model or QML updates
        QElapsedTimer timer;
        timer.start();
        while (timer.elapsed() < 2000) {
            waitEvents(50);
            if (expectedCount == 0 || (spyReset.count() > prevReset || spyRows.count() > prevRows || spyData.count() > prevData)) {
                if (model->rowCount() == expectedCount) {
                    break;
                }
            }
        }
        waitEvents(100);

        int currentReset = spyReset.count();
        int currentRows = spyRows.count();
        int currentData = spyData.count();

        std::cout << "[QML HARNESS ASSERT] " << stepName.toStdString() << ":\n";
        std::cout << "  Expected entries: " << expectedCount << "\n";
        std::cout << "  Model rowCount: " << model->rowCount() << "\n";
        std::cout << "  Signal spy counts: reset=" << currentReset << " (+" << (currentReset - prevReset)
                  << ") rows=" << currentRows << " (+" << (currentRows - prevRows)
                  << ") data=" << currentData << " (+" << (currentData - prevData)
                  << ") countChanged=" << spyCount.count() << "\n";

        bool signalFired = (currentReset > prevReset) || (currentRows > prevRows) || (currentData > prevData);
        if (expectedCount > 0 && !signalFired) {
            std::cerr << "  FAIL: Qt model did not emit rowsInserted, dataChanged, or modelReset!\n";
            return false;
        }

        bool listVisible = taskbarList->property("visible").toBool();
        int listCount = taskbarList->property("count").toInt();
        int modelCountProp = model->property("count").toInt();
        std::cout << "  Model count property: " << modelCountProp << "\n";
        std::cout << "  ListView visible property: " << (listVisible ? "true" : "false") << "\n";
        std::cout << "  ListView count: " << listCount << "\n";

        if (listVisible != (expectedCount > 0)) {
            std::cerr << "  FAIL: ListView visible property is " << (listVisible ? "true" : "false")
                      << " (expected " << (expectedCount > 0 ? "true" : "false") << ")!\n";
            return false;
        }

        if (listCount != expectedCount) {
            std::cerr << "  FAIL: ListView count is " << listCount << " (expected " << expectedCount << ")!\n";
            return false;
        }

        // Delegate Inspection
        auto *contentItem = taskbarList->property("contentItem").value<QQuickItem*>();
        QList<QQuickItem*> delegates;
        if (contentItem) {
            for (QQuickItem *child : contentItem->childItems()) {
                if (child) {
                    delegates.append(child);
                }
            }
        }
        std::cout << "  Instantiated delegates count: " << delegates.size() << "\n";

        if (expectedCount > 0) {
            if (delegates.isEmpty()) {
                std::cerr << "  FAIL: Expected " << expectedCount << " delegates, but found 0 instantiated delegates!\n";
                return false;
            }

            QRectF panelRect(0, 0, panelWindow->width(), panelWindow->height());
            QRectF clockRect;
            if (clockRow) {
                QPointF cp = clockRow->mapToScene(QPointF(0, 0));
                clockRect = QRectF(cp.x(), cp.y(), clockRow->width(), clockRow->height());
            }
            QRectF statusRect;
            if (statusArea) {
                QPointF sp = statusArea->mapToScene(QPointF(0, 0));
                statusRect = QRectF(sp.x(), sp.y(), statusArea->width(), statusArea->height());
            }

            int checkedCount = 0;
            for (int i = 0; i < delegates.size(); ++i) {
                QQuickItem *d = delegates[i];
                if (!d) continue;

                // Check isVisible()
                if (!d->isVisible()) {
                    std::cerr << "  FAIL: Delegate " << i << " isVisible() is false! Parent taskbarList visible="
                              << taskbarList->isVisible() << "\n";
                    return false;
                }

                // Check opacity
                if (d->opacity() <= 0.0) {
                    std::cerr << "  FAIL: Delegate " << i << " opacity is " << d->opacity() << " (must be > 0)!\n";
                    return false;
                }

                // Check width/height
                if (d->width() <= 0.0 || d->height() <= 0.0) {
                    std::cerr << "  FAIL: Delegate " << i << " dimensions are " << d->width() << "x" << d->height() << " (must be > 0)!\n";
                    return false;
                }

                // Check lies inside panel
                QPointF dp = d->mapToScene(QPointF(0, 0));
                QRectF dRect(dp.x(), dp.y(), d->width(), d->height());
                if (!panelRect.contains(dRect)) {
                    std::cerr << "  FAIL: Delegate " << i << " rect (" << dRect.x() << "," << dRect.y() << " " << dRect.width() << "x" << dRect.height()
                              << ") lies outside panel (" << panelRect.width() << "x" << panelRect.height() << ")!\n";
                    return false;
                }

                // Check no overlap with clock
                if (clockRow && clockRect.isValid() && dRect.intersects(clockRect)) {
                    std::cerr << "  FAIL: Delegate " << i << " overlaps centered clock!\n";
                    return false;
                }

                // Check no overlap with statusArea
                if (statusArea && statusRect.isValid() && dRect.intersects(statusRect)) {
                    std::cerr << "  FAIL: Delegate " << i << " overlaps status area!\n";
                    return false;
                }

                // Check shows non-empty label or badge
                QString text = d->property("text").toString();
                QString badge = d->property("letterBadge").toString();
                if (text.trimmed().isEmpty() && badge.trimmed().isEmpty()) {
                    std::cerr << "  FAIL: Delegate " << i << " text and letterBadge are both empty!\n";
                    return false;
                }

                checkedCount++;
            }

            if (checkedCount < expectedCount) {
                std::cerr << "  FAIL: Only verified " << checkedCount << " delegates, expected " << expectedCount << "!\n";
                return false;
            }
        }

        std::cout << "  PASS: Step " << stepName.toStdString() << " verified successfully.\n";
        return true;
    };

    // Reset feeder state before Step 0 to ensure clean initial state
    feeder.call(QStringLiteral("Reset"));
    QElapsedTimer resetTimer;
    resetTimer.start();
    while (resetTimer.elapsed() < 2000 && (model->rowCount() > 0 || taskbarList->property("count").toInt() > 0)) {
        waitEvents(50);
    }
    waitEvents(100);

    // Step 0: Initial State (Empty)
    int r0 = spyReset.count();
    int row0 = spyRows.count();
    int d0 = spyData.count();
    if (!verifyState(0, QStringLiteral("Step 0 (Initial Empty)"), r0, row0, d0)) {
        std::cerr << "[QML HARNESS FAILURE at Step 0]\n";
        return 1;
    }

    // Step 1: Added Calculator
    std::cout << "\n[QML HARNESS REPLAY] Replaying Step 1: Added Calculator...\n";
    QDBusMessage rep1 = feeder.call(
        QStringLiteral("Upsert"),
        QStringLiteral("{00de40a7-b66c-4c26-936f-8e694c56c4a5}"),
        QStringLiteral("Calculator"),
        QStringLiteral("org.gnome.Calculator"),
        false,
        true
    );
    if (rep1.type() == QDBusMessage::ErrorMessage) {
        std::cerr << "[QML HARNESS ERROR] feeder.call Upsert failed: " << rep1.errorMessage().toStdString() << std::endl;
    }
    QDBusMessage repAct = feeder.call(QStringLiteral("SetActive"), QStringLiteral("{00de40a7-b66c-4c26-936f-8e694c56c4a5}"));
    if (repAct.type() == QDBusMessage::ErrorMessage) {
        std::cerr << "[QML HARNESS ERROR] feeder.call SetActive failed: " << repAct.errorMessage().toStdString() << std::endl;
    }
    waitEvents(200);

    if (!verifyState(1, QStringLiteral("Step 1 (Added Calculator)"), r0, row0, d0)) {
        std::cerr << "[QML HARNESS FAILURE at Step 1: Added Calculator]\n";
        std::cerr << "DumpScene at failure:\n" << kynex_dump_scene_json(panelWindow).c_str() << "\n";
        return 1;
    }

    // Step 2: ActiveChanged / Unindexed window apport-gtk
    int r1 = spyReset.count();
    int row1 = spyRows.count();
    int d1 = spyData.count();
    std::cout << "\n[QML HARNESS REPLAY] Replaying Step 2: Added apport-gtk & ActiveChanged...\n";
    feeder.call(
        QStringLiteral("Upsert"),
        QStringLiteral("{d58df79b-1e65-4cf1-8cf1-df43af2ae21b}"),
        QStringLiteral("Ubuntu"),
        QStringLiteral("apport-gtk"),
        false,
        true
    );
    feeder.call(QStringLiteral("SetActive"), QStringLiteral("{d58df79b-1e65-4cf1-8cf1-df43af2ae21b}"));
    waitEvents(200);

    if (!verifyState(2, QStringLiteral("Step 2 (Added apport-gtk)"), r1, row1, d1)) {
        std::cerr << "[QML HARNESS FAILURE at Step 2: Added apport-gtk]\n";
        std::cerr << "DumpScene at failure:\n" << kynex_dump_scene_json(panelWindow).c_str() << "\n";
        return 1;
    }

    // Step 3: Changed minimized=true
    int r2 = spyReset.count();
    int row2 = spyRows.count();
    int d2 = spyData.count();
    std::cout << "\n[QML HARNESS REPLAY] Replaying Step 3: Calculator minimized=true...\n";
    feeder.call(
        QStringLiteral("Upsert"),
        QStringLiteral("{00de40a7-b66c-4c26-936f-8e694c56c4a5}"),
        QStringLiteral("Calculator"),
        QStringLiteral("org.gnome.Calculator"),
        true,
        false
    );
    waitEvents(200);

    if (!verifyState(2, QStringLiteral("Step 3 (Calculator Minimized)"), r2, row2, d2)) {
        std::cerr << "[QML HARNESS FAILURE at Step 3: Calculator Minimized]\n";
        std::cerr << "DumpScene at failure:\n" << kynex_dump_scene_json(panelWindow).c_str() << "\n";
        return 1;
    }

    // Step 4: Added Terminal
    int r3 = spyReset.count();
    int row3 = spyRows.count();
    int d3 = spyData.count();
    std::cout << "\n[QML HARNESS REPLAY] Replaying Step 4: Added Terminal...\n";
    feeder.call(
        QStringLiteral("Upsert"),
        QStringLiteral("{e78219aa-78b1-4f1e-922a-605fb31c94d2}"),
        QStringLiteral("saif@kynex-vm: ~"),
        QStringLiteral("org.gnome.Console"),
        false,
        true
    );
    feeder.call(QStringLiteral("SetActive"), QStringLiteral("{e78219aa-78b1-4f1e-922a-605fb31c94d2}"));
    waitEvents(200);

    if (!verifyState(3, QStringLiteral("Step 4 (Added Terminal)"), r3, row3, d3)) {
        std::cerr << "[QML HARNESS FAILURE at Step 4: Added Terminal]\n";
        std::cerr << "DumpScene at failure:\n" << kynex_dump_scene_json(panelWindow).c_str() << "\n";
        return 1;
    }

    std::cout << "\n[QML HARNESS] All steps passed successfully for geometry "
              << width << "x" << height << "@" << dpr << "!\n";
    return 0;
}
