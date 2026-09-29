/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#pragma once
#include <QHash>
#include <QObject>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <memory>

// Reads org.arinn.Browser1 for the shell. The screen only sees presentation
// properties; tab commands go back through this object.
class BrowserCatalog : public QObject {
  Q_OBJECT
  Q_PROPERTY(QVariantList tabs READ tabs NOTIFY tabsChanged)
  Q_PROPERTY(QVariantList windows READ windows NOTIFY windowsChanged)
  Q_PROPERTY(QVariantList shortcuts READ shortcuts NOTIFY shortcutsChanged)
  Q_PROPERTY(QString browserState READ browserState NOTIFY changed)
  Q_PROPERTY(QString operationState READ operationState NOTIFY changed)
  Q_PROPERTY(QStringList openShortcutIds READ openShortcutIds NOTIFY
                 openShortcutIdsChanged)
  Q_PROPERTY(QVariantMap currentService READ currentService NOTIFY
                 currentServiceChanged)
  Q_PROPERTY(QString retryAction READ retryAction NOTIFY changed)
  Q_PROPERTY(bool opening READ opening NOTIFY changed)
  Q_PROPERTY(QString openingTitle READ openingTitle NOTIFY changed)
  Q_PROPERTY(QString openingError READ openingError NOTIFY changed)
public:
  explicit BrowserCatalog(QObject *parent = nullptr);
  QVariantList tabs() const { return m_tabs; }
  QVariantList windows() const { return m_windows; }
  QVariantList shortcuts() const { return m_shortcuts; }
  QString browserState() const;
  QString operationState() const { return m_operation; }
  QStringList openShortcutIds() const { return m_openShortcutIds; }
  QVariantMap currentService() const { return m_currentService; }
  QString retryAction() const { return m_retryAction; }
  bool opening() const { return m_opening; }
  QString openingTitle() const { return m_openingTitle; }
  QString openingError() const { return m_openingError; }
  Q_INVOKABLE void activateTab(const QString &id);
  Q_INVOKABLE void closeTab(const QString &id);
  Q_INVOKABLE void openShortcut(const QString &id);
  Q_INVOKABLE void retry();
  Q_INVOKABLE void cancelOpen();
  Q_INVOKABLE void dismissOpenFailure();
  Q_INVOKABLE void retryOpen();
public slots:
  void refresh();
signals:
  void changed();
  void tabsChanged();
  void windowsChanged();
  void shortcutsChanged();
  void openShortcutIdsChanged();
  void currentServiceChanged();

private:
  struct Catalog;
  struct TabRef {
    QString path;
    QString generation;
    QString localId;
  };
  struct ShortcutOrigin {
    QString id;
    QString icon;
    QString scheme;
    QString host;
    int port = -1;
    bool ok = false;
  };
  void readProviders(const std::shared_ptr<Catalog> &catalog,
                     const QString &token = {});
  void readTabs(const std::shared_ptr<Catalog> &catalog, int index,
                const QString &token = {});
  void finishCatalog(const std::shared_ptr<Catalog> &catalog, bool success);
  void readShortcuts(quint64 epoch);
  void publish();
  void setOperation(const QString &state);
  void commandTab(const QString &id, const char *method);
  void watchOperation(const QString &objectPath, const QString &pendingId,
                      quint64 epoch);
  void rememberFocus(const QVariantList &windows);
  void presentTabs(QVariantList tabs);
  void clearUseOrder();
  QString m_operation;
  QVariantList m_tabs, m_windows, m_shortcuts;
  QHash<QString, TabRef> m_refs;
  QSet<QString> m_pending;
  QList<ShortcutOrigin> m_origins;
  QHash<QString, qint64> m_lastUse;
  QString m_focusedWindow;
  qint64 m_useTick = 0;
  QStringList m_openShortcutIds;
  QVariantMap m_currentService;
  QString m_retryAction = QStringLiteral("refresh");
  QString m_failedProviderId;
  bool m_opening = false;
  QString m_openingTitle, m_openingError, m_openingId, m_openOperationPath;
  bool m_openCancelRequested = false;
  bool m_connected = false, m_loaded = false, m_noBrowser = false,
       m_snapshotError = false, m_refreshing = false, m_dirty = false;
  quint64 m_epoch = 0;
  QString m_generation;
  QTimer m_timer;
};
