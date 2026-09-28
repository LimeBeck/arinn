/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include "sessionlifecycle.h"
#include <QDBusConnection>

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
  void lockChanged(bool locked);
  void prepareForSleep(bool sleeping);

private:
  void refresh();
  QDBusConnection m_session;
  bool m_locked = true, m_sleeping = false;
  quint64 m_epoch = 0;
};
