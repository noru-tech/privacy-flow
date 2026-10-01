from dataclasses import dataclass


@dataclass
class Patient:
    id: str
    religion: str
    health_record_id: str
