#include "mock_contract.h"

#include <QMetaObject>
#include <QMetaProperty>
#include <QMetaMethod>
#include <iostream>
#include <sstream>
#include <vector>
#include <string>
#include <cstring>

// Real CXX-Qt model bridges
#include "kynex-shell/src/session.cxxqt.h"
#include "kynex-shell/src/audio.cxxqt.h"
#include "kynex-shell/src/clock.cxxqt.h"
#include "kynex-shell/src/launcher.cxxqt.h"
#include "kynex-shell/src/menu.cxxqt.h"
#include "kynex-shell/src/network.cxxqt.h"
#include "kynex-shell/src/notifications.cxxqt.h"
#include "kynex-shell/src/power.cxxqt.h"
#include "kynex-shell/src/tray.cxxqt.h"
#include "kynex-shell/src/windows.cxxqt.h"

// Mock models header
#include "../../../tests/qml_tests/mocks.h"
#include "../../../tests/qml_tests/mocks.moc.cpp"

namespace {

static bool typesAreEquivalent(const char *t1, const char *t2)
{
    if (std::strcmp(t1, t2) == 0) return true;
    std::string s1(t1);
    std::string s2(t2);
    if ((s1 == "int" || s1 == "i32") && (s2 == "int" || s2 == "i32")) return true;
    if ((s1 == "double" || s1 == "f64") && (s2 == "double" || s2 == "f64")) return true;
    if (s1 == "bool" && s2 == "bool") return true;
    if (s1 == "QString" && s2 == "QString") return true;
    return false;
}

static bool isTestInstrumentationProperty(const char *name)
{
    return (std::strncmp(name, "last", 4) == 0 ||
            std::strncmp(name, "click", 5) == 0 ||
            std::strncmp(name, "middle", 6) == 0 ||
            std::strncmp(name, "close", 5) == 0 ||
            std::strncmp(name, "launch", 6) == 0 ||
            std::strncmp(name, "record", 6) == 0 ||
            std::strncmp(name, "open", 4) == 0 ||
            std::strncmp(name, "mock", 4) == 0);
}

static void compareModelContract(const char *modelName,
                                 const QMetaObject &realMeta,
                                 const QMetaObject &mockMeta,
                                 std::vector<std::string> &errors,
                                 std::ostringstream &log)
{
    log << "\n=== Verifying " << modelName << " Contract ===\n";
    log << "  Real properties (" << (realMeta.propertyCount() - realMeta.propertyOffset()) << "):\n";

    // 1. Verify every property on real model exists in mock model with matching type and notify signal
    for (int i = realMeta.propertyOffset(); i < realMeta.propertyCount(); ++i) {
        QMetaProperty realProp = realMeta.property(i);
        const char *propName = realProp.name();
        log << "    - " << propName << " (" << realProp.typeName() << ")";

        int mockIdx = mockMeta.indexOfProperty(propName);
        if (mockIdx == -1) {
            std::string err = std::string(modelName) + ": real property '" + propName + "' missing on mock model";
            errors.push_back(err);
            log << " -> MISSING ON MOCK\n";
            continue;
        }

        QMetaProperty mockProp = mockMeta.property(mockIdx);
        if (!typesAreEquivalent(realProp.typeName(), mockProp.typeName())) {
            std::string err = std::string(modelName) + ": property '" + propName + "' type mismatch: real=" +
                              realProp.typeName() + " vs mock=" + mockProp.typeName();
            errors.push_back(err);
            log << " -> TYPE MISMATCH (mock=" << mockProp.typeName() << ")\n";
            continue;
        }

        if (realProp.hasNotifySignal() != mockProp.hasNotifySignal()) {
            std::string err = std::string(modelName) + ": property '" + propName + "' notify signal existence mismatch";
            errors.push_back(err);
            log << " -> NOTIFY EXISTENCE MISMATCH\n";
            continue;
        }

        if (realProp.hasNotifySignal()) {
            QByteArray realSig = realProp.notifySignal().name();
            QByteArray mockSig = mockProp.notifySignal().name();
            if (realSig != mockSig) {
                std::string err = std::string(modelName) + ": property '" + propName + "' notify signal mismatch: real=" +
                                  realSig.constData() + " vs mock=" + mockSig.constData();
                errors.push_back(err);
                log << " -> SIGNAL MISMATCH (real=" << realSig.constData() << " vs mock=" << mockSig.constData() << ")\n";
                continue;
            }
        }

        log << " -> OK\n";
    }

    // 2. Verify that mock model does not expose spurious model properties that drift from real model
    log << "  Mock properties check against real model:\n";
    for (int i = mockMeta.propertyOffset(); i < mockMeta.propertyCount(); ++i) {
        QMetaProperty mockProp = mockMeta.property(i);
        const char *propName = mockProp.name();
        if (isTestInstrumentationProperty(propName)) {
            log << "    - " << propName << " (test instrumentation, allowed)\n";
            continue;
        }

        int realIdx = realMeta.indexOfProperty(propName);
        if (realIdx == -1) {
            std::string err = std::string(modelName) + ": mock property '" + propName +
                              "' not present in real model (mock drift defect)";
            errors.push_back(err);
            log << "    - " << propName << " -> MOCK DRIFT DEFECT (missing on real model)\n";
        } else {
            log << "    - " << propName << " -> verified against real model\n";
        }
    }
}

} // namespace

rust::String kynex_verify_mock_contracts()
{
    std::ostringstream log;
    std::vector<std::string> errors;

    log << "[MOCK CONTRACT AUDIT] Running QMetaObject contract comparison across all Kynex models...\n";

    // 1. SessionModel
    compareModelContract("SessionModel",
                         ::SessionModel::staticMetaObject,
                         MockSessionModel::staticMetaObject,
                         errors, log);

    // 2. WindowListModel
    compareModelContract("WindowListModel",
                         ::WindowListModel::staticMetaObject,
                         MockWindowListModel::staticMetaObject,
                         errors, log);

    // 3. AudioModel
    compareModelContract("AudioModel",
                         ::AudioModel::staticMetaObject,
                         MockAudioModel::staticMetaObject,
                         errors, log);

    // 4. PowerModel
    compareModelContract("PowerModel",
                         ::PowerModel::staticMetaObject,
                         MockPowerModel::staticMetaObject,
                         errors, log);

    // 5. NetworkModel
    compareModelContract("NetworkModel",
                         ::NetworkModel::staticMetaObject,
                         MockNetworkModel::staticMetaObject,
                         errors, log);

    // 6. LauncherModel
    compareModelContract("LauncherModel",
                         ::LauncherModel::staticMetaObject,
                         MockLauncherModel::staticMetaObject,
                         errors, log);

    // 7. NotificationModel
    compareModelContract("NotificationModel",
                         ::NotificationModel::staticMetaObject,
                         MockNotificationModel::staticMetaObject,
                         errors, log);

    // 8. TrayModel
    compareModelContract("TrayModel",
                         ::TrayModel::staticMetaObject,
                         MockTrayModel::staticMetaObject,
                         errors, log);

    // 9. TrayMenuModel
    compareModelContract("TrayMenuModel",
                         ::TrayMenuModel::staticMetaObject,
                         MockTrayMenuModel::staticMetaObject,
                         errors, log);

    // 10. Clock
    compareModelContract("Clock",
                         ::Clock::staticMetaObject,
                         MockClock::staticMetaObject,
                         errors, log);

    // 11. PinsModel check
    log << "\n=== Verifying PinsModel Contract ===\n";
    log << "  PinsModel: not yet implemented in shell daemon or test mocks (future roadmap scope: OK)\n";

    log << "\n=======================================================\n";
    if (errors.empty()) {
        log << "MOCK CONTRACT AUDIT PASSED: All 10 models verified with 0 drifts.\n";
        log << "=======================================================\n";
        std::string s = log.str();
        return rust::String(s.data(), s.size());
    }

    log << "MOCK CONTRACT AUDIT FAILED: " << errors.size() << " DRIFT(S) DETECTED:\n";
    for (const auto &err : errors) {
        log << "  * " << err << "\n";
    }
    log << "=======================================================\n";
    std::string s = log.str();
    return rust::String(s.data(), s.size());
}
