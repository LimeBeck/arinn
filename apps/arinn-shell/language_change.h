// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QCoreApplication>
#include <QEvent>
#include <QObject>
#include <QTimer>
#include <QString>
#include <functional>

// Synchronous preflight lets translation owners reject an invalid active
// replacement before the language manager commits a reload or reports success.
class TranslationReloadCheck final : public QEvent {
public:
  static Type typeId() {
    static const auto type = static_cast<Type>(QEvent::registerEventType());
    return type;
  }
  explicit TranslationReloadCheck(const QString &requestedLocale)
      : QEvent(typeId()), locale(requestedLocale) {}
  const QString locale;
  bool failed = false;
};

// Plain QObject models do not receive the window's LanguageChange. Observe
// the application and coalesce translator swaps into one presentation update.
class LanguageChangeObserver final : public QObject {
public:
  LanguageChangeObserver(QObject *owner, std::function<void()> update)
      : QObject(owner), m_update(std::move(update)) {
    if (auto *app = QCoreApplication::instance())
      app->installEventFilter(this);
  }
protected:
  bool eventFilter(QObject *object, QEvent *event) override {
    if (object == QCoreApplication::instance() &&
        event->type() == QEvent::LanguageChange && !m_pending) {
      m_pending = true;
      QTimer::singleShot(0, this, [this] {
        m_pending = false;
        m_update();
      });
    }
    return false;
  }
private:
  std::function<void()> m_update;
  bool m_pending = false;
};
