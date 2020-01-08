using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class UISound : MonoBehaviour
{
    public AudioClip SoundClick;
    public AudioClip SoundGold;

    public void PlayClickSound()
    {
        AudioSource.PlayClipAtPoint(SoundClick, transform.position);
    }

    public void PlayGoldSound()
    {
        AudioSource.PlayClipAtPoint(SoundGold, transform.position);
    }
}
