from flask import Flask, jsonify, request, send_from_directory
import sqlite3
import os
import time
from urllib.parse import urlparse

app = Flask(__name__, static_folder='static', static_url_path='/static')

DB_DIR = 'data'
DB_FILE = os.path.join(DB_DIR, 'bookmarks.db')


def init_db():
    if not os.path.exists(DB_DIR):
        os.makedirs(DB_DIR)

    with sqlite3.connect(DB_FILE) as conn:
        conn.execute('''
            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                title TEXT,
                url TEXT,
                folder TEXT DEFAULT ''
            )
        ''')

        # 兼容旧数据库：如果旧表没有 folder 字段，就自动加上
        cursor = conn.execute("PRAGMA table_info(bookmarks)")
        columns = [row[1] for row in cursor.fetchall()]

        if 'folder' not in columns:
            conn.execute("ALTER TABLE bookmarks ADD COLUMN folder TEXT DEFAULT ''")

        conn.commit()


def normalize_url(url):
    url = (url or '').strip()

    if not url:
        return ''

    if url.startswith(('http://', 'https://')):
        return url

    if url.startswith(('chrome://', 'edge://', 'about:', 'javascript:', 'place:')):
        return ''

    # 手动添加 google.com 这种，也自动补 https://
    return 'https://' + url


def is_valid_url(url):
    try:
        parsed = urlparse(url)
        return parsed.scheme in ('http', 'https') and bool(parsed.netloc)
    except Exception:
        return False


@app.route('/')
def index():
    return send_from_directory('.', 'index.html')


@app.route('/api/bookmarks', methods=['GET'])
def get_bookmarks():
    try:
        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.cursor()
            cursor.execute('SELECT id, title, url, folder FROM bookmarks ORDER BY folder ASC, rowid DESC')
            rows = cursor.fetchall()

            bookmarks = [
                {
                    'id': r[0],
                    'title': r[1],
                    'url': r[2],
                    'folder': r[3] or ''
                }
                for r in rows
            ]

        return jsonify(bookmarks)

    except Exception as e:
        print('GET /api/bookmarks error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks', methods=['POST'])
def save_bookmark():
    try:
        data = request.get_json(silent=True) or {}

        print('POST /api/bookmarks received:', data, flush=True)

        b_id = str(data.get('id') or int(time.time() * 1000))
        title = str(data.get('title') or '').strip()
        url = normalize_url(data.get('url'))
        folder = str(data.get('folder') or '').strip()

        if not title:
            return jsonify({'status': 'error', 'message': '标题不能为空'}), 400

        if not is_valid_url(url):
            return jsonify({'status': 'error', 'message': 'URL 无效'}), 400

        with sqlite3.connect(DB_FILE) as conn:
            duplicate = conn.execute(
                '''
                SELECT id, title, url, folder
                FROM bookmarks
                WHERE url = ? AND id != ?
                LIMIT 1
                ''',
                (url, b_id)
            ).fetchone()

            if duplicate:
                return jsonify({
                    'status': 'duplicate',
                    'message': '这个 URL 已存在',
                    'bookmark': {
                        'id': duplicate[0],
                        'title': duplicate[1],
                        'url': duplicate[2],
                        'folder': duplicate[3] or ''
                    }
                }), 409

            conn.execute(
                'INSERT OR REPLACE INTO bookmarks (id, title, url, folder) VALUES (?, ?, ?, ?)',
                (b_id, title, url, folder)
            )
            conn.commit()

            count = conn.execute('SELECT COUNT(*) FROM bookmarks').fetchone()[0]

        return jsonify({
            'status': 'success',
            'id': b_id,
            'title': title,
            'url': url,
            'folder': folder,
            'total_count': count
        })

    except Exception as e:
        print('POST /api/bookmarks error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/bulk', methods=['POST'])
def bulk_save_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        items = data.get('bookmarks', [])

        if not isinstance(items, list):
            return jsonify({'status': 'error', 'message': '导入数据格式不正确'}), 400

        valid_items = []
        skipped = 0
        duplicate_count = 0
        seen_urls = set()
        now = int(time.time() * 1000)

        with sqlite3.connect(DB_FILE) as conn:
            existing_urls = {
                row[0]
                for row in conn.execute('SELECT url FROM bookmarks WHERE url IS NOT NULL')
                if row[0]
            }

            for index, item in enumerate(items):
                if not isinstance(item, dict):
                    skipped += 1
                    continue

                b_id = str(item.get('id') or f'{now}-{index}')
                title = str(item.get('title') or '未命名书签').strip()
                url = normalize_url(item.get('url'))
                folder = str(item.get('folder') or '').strip()

                if not title or not is_valid_url(url):
                    skipped += 1
                    continue

                if url in existing_urls or url in seen_urls:
                    duplicate_count += 1
                    continue

                seen_urls.add(url)
                valid_items.append((b_id, title, url, folder))

            if valid_items:
                conn.executemany(
                    '''
                    INSERT OR REPLACE INTO bookmarks (id, title, url, folder)
                    VALUES (?, ?, ?, ?)
                    ''',
                    valid_items
                )
                conn.commit()

            total_count = conn.execute('SELECT COUNT(*) FROM bookmarks').fetchone()[0]

        return jsonify({
            'status': 'success',
            'imported_count': len(valid_items),
            'duplicate_count': duplicate_count,
            'skipped_count': skipped,
            'total_count': total_count
        })

    except Exception as e:
        print('POST /api/bookmarks/bulk error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/move', methods=['POST'])
def move_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        ids = data.get('ids', [])
        folder = ' / '.join(
            part.strip()
            for part in str(data.get('folder') or '').split('/')
            if part.strip()
        )

        if not isinstance(ids, list) or not ids:
            return jsonify({'status': 'error', 'message': '请选择要移动的书签'}), 400

        ids = [str(item).strip() for item in ids if str(item).strip()]

        if not ids:
            return jsonify({'status': 'error', 'message': '请选择要移动的书签'}), 400

        placeholders = ','.join('?' for _ in ids)

        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.execute(
                f'UPDATE bookmarks SET folder = ? WHERE id IN ({placeholders})',
                [folder, *ids]
            )
            conn.commit()

        return jsonify({
            'status': 'success',
            'moved_count': cursor.rowcount,
            'folder': folder
        })

    except Exception as e:
        print('POST /api/bookmarks/move error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/delete', methods=['POST'])
def delete_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        ids = data.get('ids', [])

        if not isinstance(ids, list) or not ids:
            return jsonify({'status': 'error', 'message': '请选择要删除的书签'}), 400

        ids = [str(item).strip() for item in ids if str(item).strip()]

        if not ids:
            return jsonify({'status': 'error', 'message': '请选择要删除的书签'}), 400

        placeholders = ','.join('?' for _ in ids)

        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.execute(
                f'DELETE FROM bookmarks WHERE id IN ({placeholders})',
                ids
            )
            conn.commit()

        return jsonify({
            'status': 'success',
            'deleted_count': cursor.rowcount
        })

    except Exception as e:
        print('POST /api/bookmarks/delete error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/<b_id>', methods=['DELETE'])
def delete_bookmark(b_id):
    try:
        with sqlite3.connect(DB_FILE) as conn:
            conn.execute('DELETE FROM bookmarks WHERE id = ?', (b_id,))
            conn.commit()

        return jsonify({'status': 'success'})

    except Exception as e:
        print('DELETE /api/bookmarks error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/folders/move-up', methods=['POST'])
def move_folder_bookmarks_up():
    try:
        data = request.get_json(silent=True) or {}
        folder = str(data.get('folder') or '').strip()

        if not folder:
            return jsonify({'status': 'error', 'message': '请选择要操作的目录'}), 400

        parts = [p.strip() for p in folder.split('/') if p.strip()]
        parent_folder = ' / '.join(parts[:-1])

        with sqlite3.connect(DB_FILE) as conn:
            rows = conn.execute(
                'SELECT id, folder FROM bookmarks WHERE folder = ? OR folder LIKE ?',
                (folder, folder + ' / %')
            ).fetchall()

            moved_count = 0

            for b_id, old_folder in rows:
                old_folder = old_folder or ''

                if old_folder == folder:
                    new_folder = parent_folder
                elif old_folder.startswith(folder + ' / '):
                    suffix = old_folder[len(folder + ' / '):]
                    new_folder = f'{parent_folder} / {suffix}' if parent_folder else suffix
                else:
                    continue

                conn.execute(
                    'UPDATE bookmarks SET folder = ? WHERE id = ?',
                    (new_folder, b_id)
                )
                moved_count += 1

            conn.commit()

        return jsonify({
            'status': 'success',
            'message': '目录已删除，书签已移动到上一层',
            'moved_count': moved_count,
            'parent_folder': parent_folder
        })

    except Exception as e:
        print('POST /api/folders/move-up error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/folders/rename', methods=['POST'])
def rename_folder():
    try:
        data = request.get_json(silent=True) or {}
        folder = str(data.get('folder') or '').strip()
        new_folder = ' / '.join(
            part.strip()
            for part in str(data.get('new_folder') or '').split('/')
            if part.strip()
        )

        if not folder:
            return jsonify({'status': 'error', 'message': '请选择要操作的目录'}), 400

        if folder == new_folder:
            return jsonify({'status': 'error', 'message': '新目录和原目录相同'}), 400

        with sqlite3.connect(DB_FILE) as conn:
            rows = conn.execute(
                'SELECT id, folder FROM bookmarks WHERE folder = ? OR folder LIKE ?',
                (folder, folder + ' / %')
            ).fetchall()

            renamed_count = 0

            for b_id, old_folder in rows:
                old_folder = old_folder or ''

                if old_folder == folder:
                    updated_folder = new_folder
                elif old_folder.startswith(folder + ' / '):
                    suffix = old_folder[len(folder + ' / '):]
                    updated_folder = f'{new_folder} / {suffix}' if new_folder else suffix
                else:
                    continue

                conn.execute(
                    'UPDATE bookmarks SET folder = ? WHERE id = ?',
                    (updated_folder, b_id)
                )
                renamed_count += 1

            conn.commit()

        return jsonify({
            'status': 'success',
            'message': '目录已更新',
            'renamed_count': renamed_count,
            'folder': folder,
            'new_folder': new_folder
        })

    except Exception as e:
        print('POST /api/folders/rename error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/folders/delete', methods=['POST'])
def delete_folder_with_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        folder = str(data.get('folder') or '').strip()

        if not folder:
            return jsonify({'status': 'error', 'message': '请选择要操作的目录'}), 400

        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.execute(
                'DELETE FROM bookmarks WHERE folder = ? OR folder LIKE ?',
                (folder, folder + ' / %')
            )
            conn.commit()

        return jsonify({
            'status': 'success',
            'message': '目录和目录下书签已删除',
            'deleted_count': cursor.rowcount
        })

    except Exception as e:
        print('POST /api/folders/delete error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


init_db()

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=7500)
