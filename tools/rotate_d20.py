from PIL import Image, ImageSequence
import random

def rotate_and_save_as_gif(image_path, output_path):
    original_image = Image.open(image_path)

    frames = []

    for _ in range(50):
        #rotated_image = original_image.rotate(random.randint(0, 360))
        rotated_image = original_image.rotate(random.randint(0, 360), expand=False, fillcolor='white')
        frames.append(rotated_image)

    frames[0].save(output_path, save_all=True, append_images=frames[1:], duration=100, loop=0)

image_path = '0.png'
output_path = 'rotated_image.gif'
rotate_and_save_as_gif(image_path, output_path)

