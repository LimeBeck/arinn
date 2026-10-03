// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once
#include "sessionlifecycle.h"
#include <QObject>
#include <QTimer>

// One startup intent per shell process, independent of later daemon reconnects.
class StartupHome : public QObject {
public:
  explicit StartupHome(SessionLifecycle *lifecycle, QObject *parent = nullptr);

private:
  void attempt();
  SessionLifecycle *m_lifecycle;
  QTimer m_retry;
  bool m_pending = false;
  bool m_done = false;
};
