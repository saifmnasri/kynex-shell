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
#include "../../crates/kynex-shell/cpp/style.h"
#include "../../crates/kynex-shell/cpp/layersurface.h"

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
    Q_INVOKABLE void sendMouseClick(QQuickItem *item, qreal x, qreal y) {
        if (!item) return;
        QPointF pos(x, y);
        QMouseEvent press(QEvent::MouseButtonPress, pos, pos, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QCoreApplication::sendEvent(item, &press);
        QMouseEvent release(QEvent::MouseButtonRelease, pos, pos, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(item, &release);
    }
};

class MockLauncherModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(int count READ count WRITE setCount NOTIFY countChanged)
    Q_PROPERTY(int selectedIndex READ selectedIndex WRITE setSelectedIndex NOTIFY selectedIndexChanged)
    Q_PROPERTY(bool visible READ visible WRITE setVisible NOTIFY visibleChanged)
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    Q_PROPERTY(int launchCallCount READ launchCallCount NOTIFY launchCallCountChanged)
    Q_PROPERTY(int lastLaunchedIndex READ lastLaunchedIndex NOTIFY lastLaunchedIndexChanged)
    Q_PROPERTY(bool closeCalled READ closeCalled NOTIFY closeCalledChanged)
    Q_PROPERTY(int closeCallCount READ closeCallCount NOTIFY closeCallCountChanged)

public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        NameRole,
        GenericNameRole,
        CommentRole,
        LetterBadgeRole,
        IconNameRole,
        IsPinnedRole
    };

    struct Item {
        QString id;
        QString name;
        QString genericName;
        QString comment;
        QString letterBadge;
        QString iconName;
        bool isPinned;
    };

    explicit MockLauncherModel(QObject *parent = nullptr)
        : QAbstractListModel(parent), m_count(0), m_selectedIndex(-1), m_visible(false),
          m_launchCallCount(0), m_lastLaunchedIndex(-1), m_closeCalled(false), m_closeCallCount(0) {}

    int rowCount(const QModelIndex &parent = QModelIndex()) const override {
        if (parent.isValid()) return 0;
        return m_items.size();
    }

    QVariant data(const QModelIndex &index, int role) const override {
        if (!index.isValid() || index.row() < 0 || index.row() >= m_items.size())
            return QVariant();
        const auto &item = m_items[index.row()];
        switch (role) {
            case IdRole: return item.id;
            case NameRole: return item.name;
            case GenericNameRole: return item.genericName;
            case CommentRole: return item.comment;
            case LetterBadgeRole: return item.letterBadge;
            case IconNameRole: return item.iconName;
            case IsPinnedRole: return item.isPinned;
            default: return QVariant();
        }
    }

    QHash<int, QByteArray> roleNames() const override {
        QHash<int, QByteArray> roles;
        roles[IdRole] = "id";
        roles[NameRole] = "name";
        roles[GenericNameRole] = "genericName";
        roles[CommentRole] = "comment";
        roles[LetterBadgeRole] = "letterBadge";
        roles[IconNameRole] = "iconName";
        roles[IsPinnedRole] = "isPinned";
        return roles;
    }

    int count() const { return m_count; }
    void setCount(int c) {
        if (m_count != c) {
            m_count = c;
            Q_EMIT countChanged();
        }
    }

    int selectedIndex() const { return m_selectedIndex; }
    void setSelectedIndex(int idx) {
        if (m_selectedIndex != idx) {
            m_selectedIndex = idx;
            Q_EMIT selectedIndexChanged();
        }
    }

    bool visible() const { return m_visible; }
    void setVisible(bool v) {
        if (m_visible != v) {
            m_visible = v;
            Q_EMIT visibleChanged();
        }
    }

    QString query() const { return m_query; }
    void setQuery(const QString &q) {
        if (m_query != q) {
            m_query = q;
            Q_EMIT queryChanged();
        }
    }

    int launchCallCount() const { return m_launchCallCount; }
    int lastLaunchedIndex() const { return m_lastLaunchedIndex; }
    bool closeCalled() const { return m_closeCalled; }
    int closeCallCount() const { return m_closeCallCount; }

    Q_INVOKABLE void addItem(const QString &id, const QString &name, const QString &genericName, const QString &comment, const QString &badge, const QString &icon = QString(), bool pinned = false) {
        beginInsertRows(QModelIndex(), m_items.size(), m_items.size());
        m_items.append({id, name, genericName, comment, badge, icon, pinned});
        endInsertRows();
        setCount(m_items.size());
    }

    Q_INVOKABLE void togglePinSelected() {
        if (m_selectedIndex >= 0 && m_selectedIndex < m_items.size()) {
            m_items[m_selectedIndex].isPinned = !m_items[m_selectedIndex].isPinned;
            Q_EMIT dataChanged(index(m_selectedIndex), index(m_selectedIndex), {IsPinnedRole});
        }
    }

    Q_INVOKABLE void clearItems() {
        beginResetModel();
        m_items.clear();
        endResetModel();
        setCount(0);
        setSelectedIndex(-1);
    }

    Q_INVOKABLE void setSearchQuery(const QString &query) {
        setQuery(query);
    }

    Q_INVOKABLE void selectPrev() {
        if (m_count <= 0) {
            setSelectedIndex(-1);
            return;
        }
        int next = m_selectedIndex - 1;
        if (next < 0) next = m_count - 1;
        setSelectedIndex(next);
    }

    Q_INVOKABLE void selectNext() {
        if (m_count <= 0) {
            setSelectedIndex(-1);
            return;
        }
        int next = m_selectedIndex + 1;
        if (next >= m_count) next = 0;
        setSelectedIndex(next);
    }

    Q_INVOKABLE void launch(int index) {
        if (index >= 0 && index < m_items.size()) {
            m_lastLaunchedIndex = index;
            m_launchCallCount++;
            Q_EMIT launchCallCountChanged();
            Q_EMIT lastLaunchedIndexChanged();
        }
    }

    Q_INVOKABLE void launchSelected() {
        if (m_selectedIndex >= 0 && m_selectedIndex < m_items.size()) {
            launch(m_selectedIndex);
        }
    }

    Q_INVOKABLE void close() {
        m_closeCalled = true;
        m_closeCallCount++;
        setVisible(false);
        Q_EMIT closeCalledChanged();
        Q_EMIT closeCallCountChanged();
    }

    Q_INVOKABLE void init() {}
    Q_INVOKABLE void recordFirstFrame() {}

    Q_INVOKABLE void resetTracking() {
        m_launchCallCount = 0;
        m_lastLaunchedIndex = -1;
        m_closeCalled = false;
        m_closeCallCount = 0;
        Q_EMIT launchCallCountChanged();
        Q_EMIT lastLaunchedIndexChanged();
        Q_EMIT closeCalledChanged();
        Q_EMIT closeCallCountChanged();
    }

Q_SIGNALS:
    void countChanged();
    void selectedIndexChanged();
    void visibleChanged();
    void queryChanged();
    void launchCallCountChanged();
    void lastLaunchedIndexChanged();
    void closeCalledChanged();
    void closeCallCountChanged();

private:
    QList<Item> m_items;
    int m_count;
    int m_selectedIndex;
    bool m_visible;
    QString m_query;
    int m_launchCallCount;
    int m_lastLaunchedIndex;
    bool m_closeCalled;
    int m_closeCallCount;
};

class MockClock : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString time READ time WRITE setTime NOTIFY timeChanged)
    Q_PROPERTY(QString date READ date WRITE setDate NOTIFY dateChanged)
public:
    explicit MockClock(QObject *parent = nullptr) : QObject(parent), m_time("12:00"), m_date("Mon Nov 7") {}
    QString time() const { return m_time; }
    void setTime(const QString &t) { if (m_time != t) { m_time = t; Q_EMIT timeChanged(); } }
    QString date() const { return m_date; }
    void setDate(const QString &d) { if (m_date != d) { m_date = d; Q_EMIT dateChanged(); } }
    Q_INVOKABLE void update() {}
    Q_INVOKABLE void init() {}
Q_SIGNALS:
    void timeChanged();
    void dateChanged();
private:
    QString m_time;
    QString m_date;
};

class MockPowerModel : public QObject {
    Q_OBJECT
    Q_PROPERTY(double percentage READ percentage WRITE setPercentage NOTIFY percentageChanged)
    Q_PROPERTY(bool charging READ charging WRITE setCharging NOTIFY chargingChanged)
    Q_PROPERTY(bool present READ present WRITE setPresent NOTIFY presentChanged)
    Q_PROPERTY(bool low READ low WRITE setLow NOTIFY lowChanged)
public:
    explicit MockPowerModel(QObject *parent = nullptr)
        : QObject(parent), m_percentage(85.0), m_charging(false), m_present(true), m_low(false) {}
    double percentage() const { return m_percentage; }
    void setPercentage(double p) { if (m_percentage != p) { m_percentage = p; Q_EMIT percentageChanged(); } }
    bool charging() const { return m_charging; }
    void setCharging(bool c) { if (m_charging != c) { m_charging = c; Q_EMIT chargingChanged(); } }
    bool present() const { return m_present; }
    void setPresent(bool pr) { if (m_present != pr) { m_present = pr; Q_EMIT presentChanged(); } }
    bool low() const { return m_low; }
    void setLow(bool l) { if (m_low != l) { m_low = l; Q_EMIT lowChanged(); } }
    Q_INVOKABLE void init() {}
Q_SIGNALS:
    void percentageChanged();
    void chargingChanged();
    void presentChanged();
    void lowChanged();
private:
    double m_percentage;
    bool m_charging;
    bool m_present;
    bool m_low;
};

class MockNetworkModel : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool present READ present WRITE setPresent NOTIFY presentChanged)
    Q_PROPERTY(bool connected READ connected WRITE setConnected NOTIFY connectedChanged)
    Q_PROPERTY(int kind READ kind WRITE setKind NOTIFY kindChanged)
    Q_PROPERTY(bool wifi READ wifi WRITE setWifi NOTIFY wifiChanged)
    Q_PROPERTY(bool ethernet READ ethernet WRITE setEthernet NOTIFY ethernetChanged)
    Q_PROPERTY(int strength READ strength WRITE setStrength NOTIFY strengthChanged)
    Q_PROPERTY(QString ssid READ ssid WRITE setSsid NOTIFY ssidChanged)
    Q_PROPERTY(bool weak READ weak WRITE setWeak NOTIFY weakChanged)
public:
    explicit MockNetworkModel(QObject *parent = nullptr)
        : QObject(parent), m_present(true), m_connected(true), m_kind(2), m_wifi(true),
          m_ethernet(false), m_strength(75), m_ssid("KynexNet"), m_weak(false) {}
    bool present() const { return m_present; }
    void setPresent(bool v) { if (m_present != v) { m_present = v; Q_EMIT presentChanged(); } }
    bool connected() const { return m_connected; }
    void setConnected(bool v) { if (m_connected != v) { m_connected = v; Q_EMIT connectedChanged(); } }
    int kind() const { return m_kind; }
    void setKind(int v) { if (m_kind != v) { m_kind = v; Q_EMIT kindChanged(); } }
    bool wifi() const { return m_wifi; }
    void setWifi(bool v) { if (m_wifi != v) { m_wifi = v; Q_EMIT wifiChanged(); } }
    bool ethernet() const { return m_ethernet; }
    void setEthernet(bool v) { if (m_ethernet != v) { m_ethernet = v; Q_EMIT ethernetChanged(); } }
    int strength() const { return m_strength; }
    void setStrength(int v) { if (m_strength != v) { m_strength = v; Q_EMIT strengthChanged(); } }
    QString ssid() const { return m_ssid; }
    void setSsid(const QString &v) { if (m_ssid != v) { m_ssid = v; Q_EMIT ssidChanged(); } }
    bool weak() const { return m_weak; }
    void setWeak(bool v) { if (m_weak != v) { m_weak = v; Q_EMIT weakChanged(); } }
    Q_INVOKABLE void init() {}
Q_SIGNALS:
    void presentChanged();
    void connectedChanged();
    void kindChanged();
    void wifiChanged();
    void ethernetChanged();
    void strengthChanged();
    void ssidChanged();
    void weakChanged();
private:
    bool m_present;
    bool m_connected;
    int m_kind;
    bool m_wifi;
    bool m_ethernet;
    int m_strength;
    QString m_ssid;
    bool m_weak;
};

class MockWindowListModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(int count READ count NOTIFY countChanged)
    Q_PROPERTY(int clickCount READ clickCount NOTIFY clickedChanged)
    Q_PROPERTY(int lastClickedIndex READ lastClickedIndex NOTIFY clickedChanged)
    Q_PROPERTY(int middleClickCount READ middleClickCount NOTIFY middleClickedChanged)
    Q_PROPERTY(int lastMiddleClickedIndex READ lastMiddleClickedIndex NOTIFY middleClickedChanged)

public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        TitleRole,
        AppIdRole,
        MinimizedRole,
        ActiveRole,
        IconNameRole,
        LetterBadgeRole,
        IsPinnedRole,
        IsRunningRole,
        WindowCountRole
    };

    struct WindowItem {
        QString id;
        QString title;
        QString appId;
        bool minimized;
        bool active;
        QString iconName;
        QString letterBadge;
        bool isPinned;
        bool isRunning;
        int windowCount;
    };

    explicit MockWindowListModel(QObject *parent = nullptr)
        : QAbstractListModel(parent), m_lastClickedIndex(-1), m_lastMiddleClickedIndex(-1),
          m_clickCount(0), m_middleClickCount(0) {}

    int rowCount(const QModelIndex &parent = QModelIndex()) const override {
        if (parent.isValid()) return 0;
        return m_items.size();
    }

    QVariant data(const QModelIndex &index, int role) const override {
        if (!index.isValid() || index.row() < 0 || index.row() >= m_items.size())
            return QVariant();
        const auto &item = m_items[index.row()];
        switch (role) {
            case IdRole: return item.id;
            case TitleRole: return item.title;
            case AppIdRole: return item.appId;
            case MinimizedRole: return item.minimized;
            case ActiveRole: return item.active;
            case IconNameRole: return item.iconName;
            case LetterBadgeRole: return item.letterBadge;
            case IsPinnedRole: return item.isPinned;
            case IsRunningRole: return item.isRunning;
            case WindowCountRole: return item.windowCount;
            default: return QVariant();
        }
    }

    QHash<int, QByteArray> roleNames() const override {
        QHash<int, QByteArray> roles;
        roles[IdRole] = "id";
        roles[TitleRole] = "title";
        roles[AppIdRole] = "appId";
        roles[MinimizedRole] = "minimized";
        roles[ActiveRole] = "active";
        roles[IconNameRole] = "iconName";
        roles[LetterBadgeRole] = "letterBadge";
        roles[IsPinnedRole] = "isPinned";
        roles[IsRunningRole] = "isRunning";
        roles[WindowCountRole] = "windowCount";
        return roles;
    }

    int count() const { return m_items.size(); }

    Q_INVOKABLE void addItem(const QString &id, const QString &title, const QString &appId,
                             bool minimized, bool active, const QString &iconName,
                             const QString &letterBadge, bool isPinned, bool isRunning, int windowCount) {
        beginInsertRows(QModelIndex(), m_items.size(), m_items.size());
        m_items.append({id, title, appId, minimized, active, iconName, letterBadge, isPinned, isRunning, windowCount});
        endInsertRows();
        Q_EMIT countChanged();
    }

    Q_INVOKABLE void clear() {
        beginResetModel();
        m_items.clear();
        endResetModel();
        Q_EMIT countChanged();
    }

    Q_INVOKABLE void init() {}
    Q_INVOKABLE void handleClick(int index) {
        m_lastClickedIndex = index;
        m_clickCount++;
        Q_EMIT clickedChanged();
    }
    Q_INVOKABLE void handleMiddleClick(int index) {
        m_lastMiddleClickedIndex = index;
        m_middleClickCount++;
        Q_EMIT middleClickedChanged();
    }
    Q_INVOKABLE void activate(const QString &) {}
    Q_INVOKABLE void minimize(const QString &) {}
    Q_INVOKABLE void restore(const QString &) {}
    Q_INVOKABLE void close(const QString &) {}

    int lastClickedIndex() const { return m_lastClickedIndex; }
    int lastMiddleClickedIndex() const { return m_lastMiddleClickedIndex; }
    int clickCount() const { return m_clickCount; }
    int middleClickCount() const { return m_middleClickCount; }

Q_SIGNALS:
    void countChanged();
    void clickedChanged();
    void middleClickedChanged();

private:
    QList<WindowItem> m_items;
    int m_lastClickedIndex;
    int m_lastMiddleClickedIndex;
    int m_clickCount;
    int m_middleClickCount;
};

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

class Setup : public QObject {
    Q_OBJECT
public:
    Setup() {}
public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
        engine->addImageProvider(QStringLiteral("kynex-icon"), new DummyIconProvider());
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
        qmlRegisterType<MockWindowListModel>("Kynex.Shell", 1, 0, "WindowListModel");
        engine->addImportPath("crates/kynex-shell/qml");
        engine->rootContext()->setContextProperty("LayoutHelper", new LayoutHelper(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(controls, Setup)
#include "main.moc"
