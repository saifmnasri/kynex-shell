// dev/test-windows.cpp - Dev-only test client for Kynex Shell window management
// Opens N windows with normal or hostile titles, and can minimize/restore/close them.
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QApplication>
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QPushButton>
#include <QLabel>
#include <QTimer>
#include <QCommandLineParser>
#include <QCommandLineOption>
#include <QList>
#include <QPointer>
#include <iostream>

class TestWindow : public QWidget {
    Q_OBJECT
public:
    explicit TestWindow(int index, const QString &title, QWidget *parent = nullptr)
        : QWidget(parent), m_index(index), m_title(title)
    {
        setWindowTitle(title);
        resize(320, 200);

        auto *layout = new QVBoxLayout(this);

        m_label = new QLabel(QString("Window #%1\nTitle: %2").arg(index).arg(title), this);
        m_label->setWordWrap(true);
        layout->addWidget(m_label);

        auto *btnLayout = new QHBoxLayout();

        auto *minBtn = new QPushButton("Minimize", this);
        connect(minBtn, &QPushButton::clicked, this, [this]() {
            showMinimized();
        });
        btnLayout->addWidget(minBtn);

        auto *renameBtn = new QPushButton("Rename", this);
        connect(renameBtn, &QPushButton::clicked, this, [this]() {
            m_title += " [Updated]";
            setWindowTitle(m_title);
            m_label->setText(QString("Window #%1\nTitle: %2").arg(m_index).arg(m_title));
        });
        btnLayout->addWidget(renameBtn);

        auto *closeBtn = new QPushButton("Close", this);
        connect(closeBtn, &QPushButton::clicked, this, [this]() {
            close();
        });
        btnLayout->addWidget(closeBtn);

        layout->addLayout(btnLayout);
    }

private:
    int m_index;
    QString m_title;
    QLabel *m_label;
};

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("Kynex Test Windows");
    app.setApplicationDisplayName("Kynex Test Windows");

    QCommandLineParser parser;
    parser.setApplicationDescription("Kynex Shell Dev-only Window Test Client");
    parser.addHelpOption();

    QCommandLineOption countOption(QStringList() << "c" << "count", "Number of windows to open", "count", "3");
    parser.addOption(countOption);

    QCommandLineOption hostileOption(QStringList() << "hostile", "Include adversarial/hostile titles");
    parser.addOption(hostileOption);

    QCommandLineOption autoOption(QStringList() << "a" << "auto", "Run automated test lifecycle sequence");
    parser.addOption(autoOption);

    parser.process(app);

    bool ok = false;
    int count = parser.value(countOption).toInt(&ok);
    if (!ok || count <= 0) {
        count = 3;
    }

    bool includeHostile = parser.isSet(hostileOption);
    bool runAuto = parser.isSet(autoOption);

    const QString hostileTitles[] = {
        QString::fromUtf8("Terminal \x1b[31;1mRed Alert\x1b[0m \x07\t\nControl"),
        QString::fromUtf8("Bidi Attack \u202E reversed text \u2066 safe \u2069 end"),
        QString("Very Long Window Title ").repeated(20),
        QString::fromUtf8("Unicode Special \u0000 Embedded Nul Test"),
        QString::fromUtf8("Special Chars <>&\"'\\/|;`$()")
    };
    const int numHostileTitles = sizeof(hostileTitles) / sizeof(hostileTitles[0]);

    QList<QPointer<TestWindow>> windows;

    for (int i = 0; i < count; ++i) {
        QString title;
        if (includeHostile) {
            title = hostileTitles[i % numHostileTitles];
        } else {
            title = QString("Test Window #%1").arg(i + 1);
        }
        auto *win = new TestWindow(i + 1, title);
        win->show();
        windows.append(win);
    }

    std::cout << "[test-windows] Opened " << count << " test windows (hostile="
              << (includeHostile ? "true" : "false") << ", auto="
              << (runAuto ? "true" : "false") << ")" << std::endl;

    if (runAuto) {
        // Lifecycle sequence:
        // t=1s: Minimize window 0
        QTimer::singleShot(1000, [&windows]() {
            if (!windows.isEmpty() && windows[0]) {
                std::cout << "[test-windows] Step 1: Minimizing window 0" << std::endl;
                windows[0]->showMinimized();
            }
        });

        // t=2s: Restore window 0
        QTimer::singleShot(2000, [&windows]() {
            if (!windows.isEmpty() && windows[0]) {
                std::cout << "[test-windows] Step 2: Restoring window 0" << std::endl;
                windows[0]->showNormal();
            }
        });

        // t=3s: Close window 1
        QTimer::singleShot(3000, [&windows]() {
            if (windows.size() > 1 && windows[1]) {
                std::cout << "[test-windows] Step 3: Closing window 1" << std::endl;
                windows[1]->close();
            }
        });

        // t=4s: Close remaining windows and exit
        QTimer::singleShot(4000, [&app, &windows]() {
            std::cout << "[test-windows] Step 4: Closing all remaining windows" << std::endl;
            for (auto &w : windows) {
                if (w) {
                    w->close();
                }
            }
            std::cout << "[test-windows] Completed lifecycle sequence. Exiting." << std::endl;
            app.quit();
        });
    }

    return app.exec();
}

#include "test-windows.moc"
