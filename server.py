from flask import Flask, request
import subprocess
import tempfile

# Initialize Flask app
app = Flask(__name__)

@app.route('/decompile', methods=['POST'])
def decompile_bytecode():
    bytecode_data = request.get_data()

    with tempfile.NamedTemporaryFile(delete=False, mode='wb') as temp_file:
        temp_file.write(bytecode_data)
        temp_file.flush()

        result = subprocess.run(['luau-lifter.exe', temp_file.name, '-e'], stdout = subprocess.PIPE, stderr = subprocess.PIPE)

        if result.returncode != 0:
            return f"Error decompiling bytecode:\n{result.stderr.decode('utf-8') or 'Unknown error occurred.'}", 400

        return result.stdout.decode('utf-8').replace('\t', ' ' * 4), 200

if __name__ == '__main__':
    app.run(host='127.0.0.1', port=1234)
