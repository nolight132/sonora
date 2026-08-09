# common
common-on = Вкл.
common-off = Выкл.
common-left = Слева
common-right = Справа
common-search = Поиск
common-unknown = Неизвестно
common-not-provided = Не указано
common-not-available = Нет данных
common-cancel = Отмена
common-save = Сохранить
common-delete = Удалить
number-group = { "\u00A0" }

# navigation
nav-home = Главная
nav-search = Поиск
nav-library = Моя медиатека
nav-settings = Настройки
nav-songs = Треки
nav-albums = Альбомы
nav-playlists = Плейлисты
library-liked-songs = Любимые треки
library-play-liked-songs = Слушать

# app menu
app-refresh-library = Обновить медиатеку
app-sign-out = Выйти
app-quit = Выход

# table columns
column-index = #
column-title = Название
column-artist = Исполнитель
column-album = Альбом
column-date-added = Дата добавления
column-length = Длительность
column-plays = Прослушивания
column-name = Название
column-owner = Владелец
column-year = Год
column-tracks = Треки

# track menu
menu-add-to-playlist = Добавить в плейлист
menu-new-playlist = Новый плейлист
menu-no-playlists = Нет плейлистов
menu-add-to-library = Добавить в медиатеку
menu-remove-from-library = Удалить из медиатеки
menu-remove-from-playlist = Удалить из плейлиста
menu-play-next = Воспроизвести следующим
menu-add-to-queue = Добавить в очередь
menu-song-radio = Радио по треку
menu-go-to-album = Перейти к альбому
menu-go-to-artist = Перейти к исполнителю
menu-view-details = Подробнее
menu-copy-link = Копировать ссылку
menu-remove-from-queue = Убрать из очереди
menu-open-playlist = Открыть плейлист
menu-play-playlist = Воспроизвести плейлист
menu-rename-playlist = Переименовать плейлист
menu-delete-playlist = Удалить плейлист
menu-remove-playlist-from-library = Удалить из медиатеки
menu-make-playlist-public = Сделать публичным
menu-make-playlist-private = Сделать приватным
menu-open-album = Открыть альбом
menu-play-album = Воспроизвести альбом
menu-add-album-to-queue = Добавить альбом в очередь

# playlist editor
playlist-name-placeholder = Название плейлиста
playlist-create-title = Создать плейлист
playlist-rename-title = Переименовать плейлист
playlist-delete-title = Удалить плейлист
playlist-delete-confirm = Удалить «{ $name }»? Это действие нельзя отменить.

# queue panel
queue-title = Очередь
queue-history = История
queue-now-playing = Сейчас играет
queue-up-next = Далее
queue-reset = Сбросить
queue-clear = Очистить
queue-empty = Очередь пуста

# player bar
player-nothing-playing = Ничего не играет
player-percent = { $value }%

# filters
filter-library = Фильтр медиатеки
filter-album = Фильтр треков альбома
filter-reset = Сбросить фильтры
filter-duration = Длительность
filter-year = Год
filter-explicit = Только с ненормативной лексикой
filter-playable = Только доступные

# view
view-list = Список
view-cards = Карточки

# login
login-signed-out = Войдите, чтобы загрузить медиатеку Spotify
login-restoring = Проверяем сохранённую сессию…
login-authorizing = Ожидаем авторизацию в браузере…
login-signed-in = Вы вошли как { $name }
login-sign-in = Войти через Spotify

# album and playlist pages
detail-album = Альбом
detail-playlist = Плейлист
detail-play-album = Слушать альбом
detail-play-playlist = Слушать плейлист

# play button
play-pause = Пауза
play-resume = Продолжить
play-loading = Загрузка…

# artist page
artist-eyebrow = Исполнитель
artist-monthly-listeners = { $count ->
    [one] { $value } слушатель в месяц
    [few] { $value } слушателя в месяц
   *[other] { $value } слушателей в месяц
}
artist-play = Слушать
artist-popular = Популярное
artist-releases = Релизы
artist-filter-all = Все
artist-filter-albums = Альбомы
artist-filter-singles = Синглы
artist-filter-eps = EP

# release kinds
release-album = Альбом
release-single = Сингл
release-compilation = Сборник
release-ep = EP
release-audiobook = Аудиокнига
release-podcast = Подкаст
release-meta = { $year } • { $kind }

# home page
home-quick-picks = Быстрый выбор
home-quick-picks-eyebrow = Начните с трека

# search page
search-placeholder = Что хотите послушать?
search-best-match = Лучшее совпадение
search-no-matches = Ничего не найдено
search-results = Результаты
search-songs = Треки
search-artists = Исполнители
search-albums = Альбомы
search-tagged = { $kind } · { $value }
search-saved =
    { $count ->
        [one] { $count } трек в медиатеке
        [few] { $count } трека в медиатеке
       *[other] { $count } треков в медиатеке
    }
kind-song = Трек
kind-artist = Исполнитель
kind-album = Альбом

# song page
song-eyebrow = Трек
song-play = Слушать трек
song-view-album = Открыть альбом
song-loading = Загружаем информацию о треке…
song-about = О треке
song-album = Альбом
song-released = Дата выхода
song-streams = Прослушивания
song-position = Позиция
song-label = Лейбл
song-popularity = Популярность
song-popularity-value = { $value }%
song-disc-track = Диск { $disc }, трек { $track }
song-track = Трек { $track }
song-credits = Авторы
song-performed-by = Исполнение
song-details = Жанры и детали
song-genres = Жанры
song-language = Язык
song-content = Контент
song-explicit = Ненормативная лексика
song-clean = Без ограничений
song-about-artist = Об исполнителе
song-artist-fallback = Послушайте популярные треки и релизы исполнителя.
song-copyright = © { $notice }

# song languages
language-ar = Арабский
language-de = Немецкий
language-en = Английский
language-es = Испанский
language-fr = Французский
language-hi = Хинди
language-it = Итальянский
language-ja = Японский
language-ko = Корейский
language-pt = Португальский
language-ru = Русский
language-tr = Турецкий
language-uk = Украинский
language-zh = Китайский
language-zxx = Без слов

# counts
count-songs =
    { $count ->
        [one] { $count } трек
        [few] { $count } трека
       *[other] { $count } треков
    }

# dates
date-full = { $day } { $month } { $year }
month-1 = янв.
month-2 = фев.
month-3 = мар.
month-4 = апр.
month-5 = мая
month-6 = июн.
month-7 = июл.
month-8 = авг.
month-9 = сен.
month-10 = окт.
month-11 = ноя.
month-12 = дек.

# settings
settings-tab-appearance = Внешний вид
settings-tab-playback = Воспроизведение
settings-tab-account = Аккаунт
settings-theme = Тема
settings-theme-detail = Цветовая палитра приложения
settings-theme-config = Открыть конфиг
settings-adaptive = Адаптивная тема
settings-adaptive-detail = Подкрашивать палитру обложкой играющего альбома
settings-corners = Углы
settings-corners-detail = Насколько скруглены поверхности и элементы
settings-font = Размер шрифта
settings-font-detail = Базовый размер текста, остальное масштабируется вместе с ним
settings-font-value = { $size } px
settings-language = Язык
settings-language-detail = Язык интерфейса sonora
settings-language-system = Системный
settings-auto-hide = Автоскрытие боковой панели
settings-auto-hide-detail = Сворачивать боковую панель, когда окно становится узким
settings-window-controls = Кнопки окна
settings-window-controls-detail = Рисовать свернуть, развернуть и закрыть в заголовке окна
settings-controls-side = Сторона кнопок
settings-controls-side-detail = С какой стороны заголовка расположены кнопки
settings-normalisation = Нормализация громкости
settings-normalisation-detail = Держит треки на одинаковой громкости
settings-account = Аккаунт
settings-account-detail = Выйти из Spotify на этом устройстве
settings-sign-out = Выйти
settings-tab-about = О программе
settings-version = Версия
settings-version-detail = Сборка sonora, которая сейчас запущена
settings-license = Лицензия
settings-license-detail = GNU General Public License версии 3 или новее
settings-license-view = Прочитать лицензию
settings-source = Исходный код
settings-source-detail = Исходный код, соответствующий этой сборке
settings-source-view = Открыть репозиторий
settings-team = Команда
settings-team-github = GitHub
settings-role-lead-maintainer = Ведущий сопровождающий
settings-role-maintainer = Сопровождающий
settings-role-contributor = Участник
settings-notice = Copyright © 2026 nolight132. Sonora поставляется без каких-либо гарантий. Это свободное программное обеспечение, и вы можете распространять его на условиях GNU General Public License версии 3 или новее. Sonora — неофициальный клиент и не связан со Spotify AB.

# themes
theme-dark = Тёмная
theme-light = Светлая
theme-midnight = Полночь
theme-forest = Лес
theme-ocean = Океан
theme-rose = Роза
theme-lavender = Лаванда
theme-amber = Янтарь

# corners
corners-square = Прямые
corners-subtle = Лёгкие
corners-rounded = Скруглённые
corners-round = Круглые

toast-playlist-created = Плейлист создан
toast-playlist-renamed = Плейлист переименован
toast-playlist-deleted = Плейлист удалён
toast-playlist-removed = Плейлист удалён из медиатеки
toast-playlist-visibility = Видимость плейлиста изменена
toast-track-added = Трек добавлен в плейлист
toast-playlist-failed = Не удалось сохранить изменение
toast-playlist-busy = Другое изменение ещё выполняется
toast-playlist-signed-out = Войдите, чтобы менять плейлисты
