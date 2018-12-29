using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class TestSkill03 : MonoBehaviour
{
    public DoSkill DS;
    public SelfExplodeScript SES;
    private float timetosing = 1;
    private float timesinged;

    // Use this for initialization
    void Start()
    {
        timesinged = 0;
    }

    public void Go()
    {
        if (SES.skillavaliable)
        {
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (DS.singing != 3)
        {
            timesinged = 0;
            return;
        }
        else
        {
            timesinged += Time.fixedDeltaTime;
            if (timesinged >= timetosing)
            {
                gameObject.GetComponent<SelfExplodeScript>().Skill();
                timesinged = 0;
                GetComponent<DoSkill>().singing = 0;
            }
        }
    }

    public void Skill()
    {
        DS.BeforeSkill();
        DS.singing = 3;
    }
}
