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
        LetterBadgeRole
    };

    struct Item {
        QString id;
        QString name;
        QString genericName;
        QString comment;
        QString letterBadge;
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

    Q_INVOKABLE void addItem(const QString &id, const QString &name, const QString &genericName, const QString &comment, const QString &badge) {
        beginInsertRows(QModelIndex(), m_items.size(), m_items.size());
        m_items.append({id, name, genericName, comment, badge});
        endInsertRows();
        setCount(m_items.size());
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

class Setup : public QObject {
    Q_OBJECT
public:
    Setup() {}
public slots:
    void qmlEngineAvailable(QQmlEngine *engine) {
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
        engine->addImportPath("crates/kynex-shell/qml");
        engine->rootContext()->setContextProperty("LayoutHelper", new LayoutHelper(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(controls, Setup)
#include "main.moc"
