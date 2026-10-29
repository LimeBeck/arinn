// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QtCore/QAnimationDriver>
#include <QtCore/QElapsedTimer>
#include <QtCore/QObject>
#include <QtCore/QTimer>
#include <QtCore/QtGlobal>
#include <QtGui/QGuiApplication>
#include <QtGui/QStyleHints>
#include <QtQml/QQmlContext>
#include <QtQml/QQmlEngine>
#include <QtQuickTest/quicktest.h>

namespace {

// Capture mode only (ui-compare sets ARINN_CAPTURE_STILL_ANIMATIONS).
// Animation time, which also drives QML Timers, follows the wall clock until
// the capture calls captureAnimation.hold(). From then on it stands still
// unless the capture runs it: run(ms) moves it forward by exactly ms in one
// jump, whatever the machine's speed, so each frame shows the same moment on
// every run. release() hands time back to the wall clock.
class CaptureAnimationDriver : public QAnimationDriver
{
public:
    CaptureAnimationDriver()
    {
        m_timer.setInterval(1);
        QObject::connect(&m_timer, &QTimer::timeout, this, [this] {
            if (!m_held) {
                m_time = m_wall.elapsed() + m_drift;
                advance();
                return;
            }
            if (m_budget <= 0)
                return;
            // One jump: QUnifiedTimer applies the whole delta in a single tick.
            m_time += m_budget;
            m_budget = 0;
            advance();
        });
        QObject::connect(this, &QAnimationDriver::started, this, [this] {
            m_time = 0;
            m_drift = 0;
            m_wall.start();
            m_timer.start();
        });
        QObject::connect(this, &QAnimationDriver::stopped, &m_timer, &QTimer::stop);
    }

    qint64 elapsed() const override { return isRunning() ? m_time : 0; }
    void run(int ms) { m_budget += qMax(0, ms); }
    void hold(bool held)
    {
        m_held = held;
        m_budget = 0;
        if (!held && m_wall.isValid())
            m_drift = m_time - m_wall.elapsed();
    }
    bool idle() const { return m_budget <= 0; }

private:
    QTimer m_timer;
    QElapsedTimer m_wall;
    bool m_held = false;
    qint64 m_drift = 0;
    qint64 m_time = 0;
    qint64 m_budget = 0;
};

CaptureAnimationDriver *captureDriver = nullptr;

} // namespace

// Exposed to QML as captureAnimation; a no-op outside capture mode.
class CaptureAnimation : public QObject
{
    Q_OBJECT

public:
    using QObject::QObject;
    Q_INVOKABLE void run(int ms)
    {
        if (captureDriver)
            captureDriver->run(ms);
    }
    Q_INVOKABLE void hold()
    {
        if (captureDriver)
            captureDriver->hold(true);
    }
    Q_INVOKABLE void release()
    {
        if (captureDriver)
            captureDriver->hold(false);
    }
    Q_INVOKABLE bool idle() const { return !captureDriver || captureDriver->idle(); }
};

class ArinnUiTestSetup : public QObject
{
    Q_OBJECT

public slots:
    void applicationAvailable()
    {
        if (!qEnvironmentVariableIsSet("ARINN_CAPTURE_STILL_ANIMATIONS"))
            return;
        captureDriver = new CaptureAnimationDriver;
        captureDriver->install();
        // A blinking text cursor runs on wall time; keep it on.
        QGuiApplication::styleHints()->setCursorFlashTime(0);
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        engine->rootContext()->setContextProperty(QStringLiteral("captureAnimation"),
                                                  new CaptureAnimation(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(arinn_ui, ArinnUiTestSetup)

#include "main.moc"
