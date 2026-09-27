/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include <QDBusConnection>
#include <QObject>

// UI visibility follows session lock and sleep independently of daemon state.
class SessionGuard : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.arinn.SpikeSession1")
  Q_PROPERTY(bool blocked READ blocked NOTIFY changed)
public:
  explicit SessionGuard(QObject *parent = nullptr,
                        QDBusConnection session = QDBusConnection::sessionBus(),
                        QDBusConnection system = QDBusConnection::systemBus());
  bool blocked() const { return m_locked || m_sleeping; }
signals:
  void changed();
private slots:
  void lockChanged(bool locked);
  void prepareForSleep(bool sleeping);

private:
  void refresh();
  QDBusConnection m_session;
  bool m_locked = true, m_sleeping = false;
  quint64 m_epoch = 0;
};
