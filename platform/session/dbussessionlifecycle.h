/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include "sessionlifecycle.h"
#include <QDBusConnection>
#include <QDBusMessage>

// UI visibility follows session lock and sleep independently of daemon state.
class DbusSessionLifecycle : public SessionLifecycle {
  Q_OBJECT
public:
  explicit DbusSessionLifecycle(
      QObject *parent = nullptr,
      QDBusConnection session = QDBusConnection::sessionBus(),
      QDBusConnection system = QDBusConnection::systemBus());
  bool blocked() const override { return m_locked || m_sleeping; }
private slots:
  void lockChanged(bool locked, const QDBusMessage &message);
  void prepareForSleep(bool sleeping);

private:
  void refresh();
  void probe(int index, quint64 epoch);
  QDBusConnection m_session;
  bool m_locked = true, m_sleeping = false;
  quint64 m_epoch = 0;
  int m_endpoint = -1;
};
