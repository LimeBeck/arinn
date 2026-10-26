// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// #363: the shell's Russian translation is built into the resources and
// loads for a Russian locale; English stays the fallback.
#include <QCoreApplication>
#include <QLocale>
#include <QTest>
#include <QTranslator>

class TranslationsTest : public QObject {
  Q_OBJECT
private slots:
  void russianLoadsForARussianLocale() {
    QTranslator translator;
    QVERIFY(translator.load(QLocale(QStringLiteral("ru_RU")), QStringLiteral("arinn"),
                            QStringLiteral("_"), QStringLiteral(":/i18n")));
    QVERIFY(QCoreApplication::installTranslator(&translator));
    // QML contexts are file names; C++ contexts are class names.
    QCOMPARE(QCoreApplication::translate("HomeView", "Continue"), QStringLiteral("Продолжить"));
    QCOMPARE(QCoreApplication::translate("HomeView", "Favorites"), QStringLiteral("Избранное"));
    QCOMPARE(QCoreApplication::translate("Main", "Home could not be loaded"),
             QStringLiteral("Не удалось загрузить главную"));
    // Restarting Arinn is not the power menu's computer restart (ADR-0098).
    QCOMPARE(QCoreApplication::translate("HomeView", "Restart"), QStringLiteral("Перезапустить"));
    QCOMPARE(QCoreApplication::translate("PowerMenu", "Restart"), QStringLiteral("Перезагрузка"));
    // A folder or path leaves the list; nothing is deleted (§5.6, handoff
    // LF-6: «Убрать», never «Удалить»; #558).
    QCOMPARE(QCoreApplication::translate("SettingsView", "Remove"), QStringLiteral("Убрать"));
    QCOMPARE(QCoreApplication::translate("SettingsCatalog", "Media"), QStringLiteral("Медиа"));
    // Library tabs are named by the shell, not by the provider (#431).
    QCOMPARE(QCoreApplication::translate("LibraryCatalog", "Films"), QStringLiteral("Фильмы"));
    QCOMPARE(QCoreApplication::translate("LibraryCatalog", "Series"), QStringLiteral("Сериалы"));
    QCOMPARE(QCoreApplication::translate("LibraryCatalog", "Other"), QStringLiteral("Другое"));
    QCOMPARE(QCoreApplication::translate("DownloadCatalog", "%1 is ready to watch")
                 .arg(QStringLiteral("X")),
             QStringLiteral("X загружен — можно смотреть"));
    QCOMPARE(QCoreApplication::translate("QObject", "%1 h %2 min left")
                 .arg(1).arg(32),
             QStringLiteral("Осталось 1 ч 32 мин"));
    // Counted strings use Qt's numerus forms.
    for (const int n : {1, 3, 5})
      QCOMPARE(QCoreApplication::translate("HomeView", "%n more open", nullptr, n),
               QStringLiteral("Открыто ещё: %1").arg(n));
    QCOMPARE(QCoreApplication::translate("DownloadsView", "%n downloading", nullptr, 2),
             QStringLiteral("Загружается: 2"));
    QCoreApplication::removeTranslator(&translator);
  }

  void englishIsTheFallback() {
    QTranslator translator;
    QVERIFY(!translator.load(QLocale(QStringLiteral("de_DE")), QStringLiteral("arinn"),
                             QStringLiteral("_"), QStringLiteral(":/i18n")));
    QCOMPARE(QCoreApplication::translate("HomeView", "Continue"), QStringLiteral("Continue"));
    QCOMPARE(QCoreApplication::translate("HomeView", "%n more open", nullptr, 2),
             QStringLiteral("2 more open"));
  }
};

QTEST_GUILESS_MAIN(TranslationsTest)
#include "translations_test.moc"
