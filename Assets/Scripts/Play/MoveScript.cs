using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public delegate void CookVelo(Rigidbody2D victim, MoveScript worker);

public class MoveScript : MonoBehaviour
{

    public bool controllable;
    //public bool cooking;
    public float movespeed;
    public Vector2 movedirection = Vector2.zero;
    public GameObject targetshadow;
    public Fix64Vector2 Givenvelocity;
    public Vector2 VelotoAdd = Vector2.zero;
    private Vector2 movetarget;
    public Vector2 selfvelocity;
    public Rigidbody2D PlayerRb2d;
    public bool isme = false;
    GameObject targeticon;
    private Vector3 followplace;
    public CookVelo cook;
    public bool fixposlater = false;

    void cookstart(Rigidbody2D victim, MoveScript worker)
    {
        worker.VelotoAdd = Vector2.zero;
    }

    // Use this for initialization
    void Start()
    {
        PlayerRb2d = GetComponent<Rigidbody2D>();
        controllable = true;
        cook = cookstart;
    }

    void Setselfvelocity()
    {
        if (targeticon != null)
        {
            movetarget = targeticon.transform.position;
            movedirection = movetarget - PlayerRb2d.position;
            if (movedirection.sqrMagnitude < 0.1)
            {
                stopwalking();
            }
            selfvelocity = movedirection.normalized * movespeed;// + StealthScript.Speed);
        }
        else
        {
            selfvelocity = Vector2.zero;
        }
    }

    public void itsme()
    {
        isme = true;
        GameObject.Find("Canvas2").GetComponentInChildren<SkillsLink>().linktome(gameObject);
        GetComponent<SpriteRenderer>().color = Color.green;
    }

    public void SetTarget(Vector2 rcplace)
    {
        GameObject.Destroy(targeticon);
        targeticon = Instantiate(targetshadow, rcplace, Quaternion.identity);
        if (isme)
            targeticon.GetComponent<SpriteRenderer>().color = new Color(1, 1, 1, 0.5f);
        GetComponent<DoSkill>().singing = 0;
        GetComponent<SkillR2b>().IdoDSWL();
        GetComponent<SkillC4>().DoFake(rcplace);
    }

    void FixedUpdate()
    {
        Setselfvelocity();
        SetTotalVelocity();
    }

    void SetTotalVelocity()
    {
        if (controllable)
        {
            cook(PlayerRb2d, this);
            PlayerRb2d.velocity = selfvelocity + VelotoAdd;
        }
        else
        {
            PlayerRb2d.velocity = Givenvelocity.ToV2();
        }
    }

    private void LateUpdate()
    {
        if (Input.GetKeyDown(KeyCode.Alpha2) && isme)
        {
            GameObject maincam = GameObject.Find("Main Camera");
            followplace = new Vector3(PlayerRb2d.position.x, PlayerRb2d.position.y, maincam.transform.position.z);
            maincam.transform.position = followplace;
        }
    }

    public void stopwalking()
    {
        GameObject.Destroy(targeticon);
        //GetComponent<SkillR2b>().IdoDSWL();
    }

    private void OnDestroy()
    {
        Destroy(targeticon);
        if (isme)
            GameObject.Find("Canvas2").GetComponentInChildren<SkillsLink>().BottomPanel.SetActive(false);
    }
}
