// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QtCore/QAnimationDriver>
#include <QtCore/QObject>
#include <QtCore/QTimer>
#include <QtCore/QtGlobal>
#include <QtQuickTest/quicktest.h>

namespace {

// Capture mode only (ui-compare sets ARINN_CAPTURE_STILL_ANIMATIONS): every
// tick advances animation time by a whole common period of the shell's
// looping animations (900, 1400, 800 ms). Finite transitions finish on their
// first tick and loops always sit at their start, so a frame never shows a
// transition half-way and two captures are identical.
class StillAnimationDriver : public QAnimationDriver
{
public:
    static constexpr qint64 kStepMs = 50400;

    StillAnimationDriver()
    {
        m_timer.setInterval(16);
        QObject::connect(&m_timer, &QTimer::timeout, this, [this] {
            ++m_ticks;
            advance();
        });
        QObject::connect(this, &QAnimationDriver::started, this, [this] {
            m_ticks = 0;
            m_timer.start();
        });
        QObject::connect(this, &QAnimationDriver::stopped, &m_timer, &QTimer::stop);
    }

    qint64 elapsed() const override { return isRunning() ? m_ticks * kStepMs : 0; }

private:
    QTimer m_timer;
    qint64 m_ticks = 0;
};

} // namespace

class ArinnUiTestSetup : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        if (qEnvironmentVariableIsSet("ARINN_CAPTURE_STILL_ANIMATIONS"))
            (new StillAnimationDriver)->install();
    }
};

QUICK_TEST_MAIN_WITH_SETUP(arinn_ui, ArinnUiTestSetup)

#include "main.moc"
