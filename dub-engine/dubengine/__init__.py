"""dub-engine — reusable local video dubbing/translation engine over an editable Project document."""
__version__ = "0.1.0"

from .opts import EngineOpts
from .progress import Progress, ProgressEvent, default_logger
from .project import Project, Segment, SubStyle, Captions, BlurBox, Title, Brand
from .api import (analyze, render, preview_frame, source_frame,
                  translate, rewrite, recast, edit_caption, edit_blur, edit_segment, del_segment, add_blur, del_blur, set_mode,
                  edit_title, del_title, add_title)
from .download import ensure_mt_model

__all__ = ["EngineOpts", "Project", "Segment", "SubStyle", "Captions", "BlurBox", "Title", "Brand",
           "Progress", "ProgressEvent", "default_logger",
           "analyze", "render", "preview_frame", "source_frame",
           "translate", "rewrite", "recast", "edit_caption", "edit_blur", "edit_segment", "del_segment",
           "add_blur", "del_blur", "set_mode", "edit_title", "del_title", "add_title", "ensure_mt_model"]
