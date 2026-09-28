// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once
#include <QObject>

// Platform-neutral visibility state; unknown state must remain blocked.
class SessionLifecycle : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.arinn.Session1")
  Q_PROPERTY(bool blocked READ blocked NOTIFY changed)
public:
  using QObject::QObject;
  virtual bool blocked() const = 0;
signals:
  void changed();
};

SessionLifecycle *createSessionLifecycle(QObject *parent);
