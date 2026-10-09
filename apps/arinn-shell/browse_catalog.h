// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QVariantList>

// Browse1 owns resolution and history; this object only projects daemon
// replies.
class BrowseCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString text READ text WRITE setText NOTIFY changed)
  Q_PROPERTY(QString kind READ kind NOTIFY changed)
  Q_PROPERTY(QString url READ url NOTIFY changed)
  Q_PROPERTY(QString engine READ engine NOTIFY changed)
  Q_PROPERTY(QString openTitle READ openTitle NOTIFY changed)
  Q_PROPERTY(QString operationState READ operationState NOTIFY changed)
  Q_PROPERTY(QStringList recent READ recent NOTIFY changed)
  Q_PROPERTY(QVariantList suggestions READ suggestions NOTIFY changed)
  Q_PROPERTY(QVariantList tabs READ tabs WRITE setTabs NOTIFY changed)
public:
  explicit BrowseCatalog(QObject *parent = nullptr);
  QString text() const { return m_text; }
  QString kind() const { return m_kind; }
  QString url() const { return m_url; }
  QString engine() const { return m_engine; }
  QString openTitle() const;
  QString operationState() const { return m_operation; }
  QStringList recent() const { return m_recent; }
  QVariantList suggestions() const;
  QVariantList tabs() const { return m_tabs; }
  void setTabs(const QVariantList &tabs);
  void setText(const QString &text);
  // Open services whose title or host matches the text, up to four; the
  // value is the origin (KB-1, KB-2).
  Q_INVOKABLE QVariantList openServices(const QString &text) const;
  // Names for a web shortcut at this address: the title of the open page
  // there and the host, those that start with the text (KB-3).
  Q_INVOKABLE QVariantList nameSuggestions(const QString &address,
                                           const QString &text) const;
  Q_INVOKABLE void open();
  Q_INVOKABLE void reset();
  Q_INVOKABLE void clearRecent();
  Q_INVOKABLE void dismissError();
public slots:
  void refreshRecent();
signals:
  void changed();

private:
  void watchOperation(const QString &path, quint64 ticket, int polls = 0);
  QString m_text, m_kind, m_url, m_engine, m_operation;
  QStringList m_recent;
  QVariantList m_tabs;
  quint64 m_resolve = 0, m_recentRead = 0, m_command = 0;
  bool m_busy = false;
};
