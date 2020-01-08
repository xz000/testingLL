using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class UISound : MonoBehaviour
{
    public AudioClip SoundClick;

    public void PlayClickSound()
    {
        AudioSource.PlayClipAtPoint(SoundClick, transform.position);
    }
}
