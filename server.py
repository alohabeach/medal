from flask import Flask, request
import random
import os
import subprocess

# Constants
ALPHANUMERIC_CHARS = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
TEMP_FILENAME_PREFIX = 'temp_'
LUAU_LIFTER_EXECUTABLE = 'luau-lifter.exe'

# Initialize Flask app
app = Flask(__name__)

def generate_random_filename(prefix, length=32, extension='.bin'):
    """Generate a random filename with the given prefix, length, and extension."""
    random_suffix = ''.join(random.choices(ALPHANUMERIC_CHARS, k=length))
    return f"{prefix}{random_suffix}{extension}"

def replace_tabs_with_spaces(input_string, spaces_per_tab=4):
    """Replace all tabs in a string with spaces."""
    return input_string.replace('\t', ' ' * spaces_per_tab)

@app.route('/decompile', methods=['POST'])
def decompile_bytecode():
    """Handle bytecode decompilation requests."""
    bytecode_data = request.get_data()
    temp_filename = generate_random_filename(TEMP_FILENAME_PREFIX)

    try:
        # Write bytecode to temporary file
        with open(temp_filename, 'wb') as temp_file:
            temp_file.write(bytecode_data)

        # Run the decompiler
        result = subprocess.run(
            [LUAU_LIFTER_EXECUTABLE, temp_filename, '-e'], 
            stdout=subprocess.PIPE, 
            stderr=subprocess.PIPE
        )

        if result.returncode != 0:
            error_message = result.stderr.decode('utf-8') or 'Unknown error occurred.'
            app.logger.error(f"Error decompiling bytecode: {error_message} (File: {temp_filename})")
            return f"Error decompiling bytecode:\n{error_message}", 400

        app.logger.info(f"Decompiled bytecode successfully: {temp_filename}")
        return replace_tabs_with_spaces(result.stdout.decode('utf-8')), 200

    finally:
        # Clean up temporary file
        if os.path.exists(temp_filename):
            os.remove(temp_filename)

if __name__ == '__main__':
    app.run(host='127.0.0.1', port=1234)
