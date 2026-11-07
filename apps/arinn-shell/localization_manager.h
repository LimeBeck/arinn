// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QDateTime>
#include <QLocale>
#include <QObject>
#include <QPointer>
#include <QStringList>
#include <QTranslator>
#include <memory>
#include <vector>
class QQmlEngine;

class LocalizationManager final : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString language READ language NOTIFY changed)
  Q_PROPERTY(QString translationLocale READ translationLocale NOTIFY changed)
  Q_PROPERTY(QString formatLocale READ formatLocale NOTIFY changed)
  Q_PROPERTY(qulonglong revision READ revision NOTIFY changed)
  Q_PROPERTY(bool reloadFailed READ reloadFailed NOTIFY changed)
public:
  explicit LocalizationManager(QObject *parent = nullptr);
  LocalizationManager(QStringList roots, QString cacheFile, QLocale systemLocale,
                      QObject *parent = nullptr);
  ~LocalizationManager() override;
  QString language() const { return m_language; }
  QString translationLocale() const { return m_translationLocale; }
  QString formatLocale() const { return m_formatLocale.bcp47Name(); }
  qulonglong revision() const { return m_revision; }
  bool reloadFailed() const { return m_reloadFailed; }
  void attachEngine(QQmlEngine *engine);
  Q_INVOKABLE void setLanguage(const QString &language);
  Q_INVOKABLE void reload();
  Q_INVOKABLE QString clockText(const QDateTime &time) const;
signals:
  void changed();
private:
  struct LoadedTranslation {
    QString path;
    QByteArray bytes;
    std::unique_ptr<QTranslator> translator;
  };
  void apply();
  QStringList m_roots;
  QString m_cacheFile;
  QLocale m_systemLocale;
  QLocale m_formatLocale;
  QString m_language = QStringLiteral("system");
  QString m_translationLocale;
  QPointer<QQmlEngine> m_engine;
  std::vector<std::unique_ptr<LoadedTranslation>> m_translators;
  QString m_appliedLanguage;
  bool m_reloadFailed = false;
  qulonglong m_revision = 0;
};
